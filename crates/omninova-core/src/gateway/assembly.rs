//! The single place where a runnable `Agent` is put together.
//!
//! Every entry point (inbound, streaming inbound, one-shot `chat`, interactive
//! terminal) goes through `assemble_agent`, so provider selection, tool
//! construction, skill injection and prompt assembly cannot drift apart the way
//! four hand-copied versions did.

use super::{
    apply_skills_for_request, attach_delegate_tool, resolve_agent_max_tool_iterations,
    ExecutionStep, GatewayRuntime,
};
use crate::agent::Agent;
use crate::channels::ChannelKind;
use crate::config::Config;
use crate::knowledge::append_knowledge_prompt;
use crate::providers::{build_provider_with_selection, ProviderSelection};
use crate::routing::RouteDecision;
use crate::security::SecurityContext;
use crate::skills::SkillInvocation;
use crate::tools::{build_tools, Tool, ToolBuildContext};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub(crate) struct AssembledAgent {
    pub agent: Agent,
    pub browser_takeover: Option<crate::tools::browser_runtime::BrowserTakeoverHandle>,
}

/// Everything that varies between entry points. Anything not listed here is
/// derived from config so it stays identical across paths.
pub(crate) struct AgentAssemblyRequest<'a> {
    pub route: &'a RouteDecision,
    pub channel: &'a ChannelKind,
    /// Exact active Agent run identity. Non-executing projection/enumeration
    /// callers leave this unset, so Personal Chrome cannot be authorized.
    pub run_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub workspace: &'a Path,
    pub spawn_depth: u32,
    pub skill_invocations: &'a [SkillInvocation],
    pub security: &'a SecurityContext,
}

impl GatewayRuntime {
    pub(crate) async fn assemble_agent(
        &self,
        cfg: &Config,
        request: &AgentAssemblyRequest<'_>,
        steps: &mut Vec<ExecutionStep>,
    ) -> anyhow::Result<AssembledAgent> {
        let agent_name = request.route.agent_name.as_str();

        let provider = build_provider_with_selection(
            cfg,
            &ProviderSelection {
                provider: request.route.provider.clone(),
                model: request.route.model.clone(),
            },
        );

        let memory = self.memory().await;
        let browser_takeover_slot = Arc::new(Mutex::new(None));
        let personal_chrome = self.personal_chrome_factory_context(
            request.run_id,
            cfg.permissions.is_full_access(),
        );
        let mut tools = build_tools(&ToolBuildContext {
            config: cfg,
            workspace: request.workspace,
            memory: Some(&memory),
            session_id: request.session_id,
            browser_takeover_slot: Some(&browser_takeover_slot),
            personal_chrome: personal_chrome.as_ref(),
        });
        apply_agent_tool_allowlist(cfg, agent_name, &mut tools);

        if attach_delegate_tool(
            cfg,
            self,
            agent_name,
            request.session_id,
            request.channel,
            request.spawn_depth,
            request.workspace,
            &mut tools,
        ) {
            steps.push(ExecutionStep::done("加载委托工具", "已启用 delegate 工具"));
        }
        steps.push(ExecutionStep::done(
            "加载工具",
            format!("可用工具数：{}", tools.len()),
        ));

        let mut agent_cfg = cfg.agent.clone();
        if let Some(delegate) = cfg.agents.get(agent_name) {
            if let Some(prompt) = &delegate.system_prompt {
                agent_cfg.system_prompt = Some(prompt.clone());
            }
        }
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(cfg, agent_name);
        if cfg.computer_use.enabled {
            agent_cfg.max_tool_iterations = agent_cfg
                .max_tool_iterations
                .max(cfg.computer_use.max_tool_iterations);
        }
        if request.security.inbound_task_id().is_some() {
            agent_cfg.planning.enabled = true;
        }

        append_workspace_note(
            &mut agent_cfg.system_prompt,
            request.workspace,
            cfg.permissions.is_full_access(),
        );

        let skill_runtime = apply_skills_for_request(
            cfg,
            request.skill_invocations,
            &mut tools,
            &mut agent_cfg.system_prompt,
        );
        if !request.skill_invocations.is_empty() && skill_runtime.activated.is_empty() {
            let reason = skill_runtime
                .errors
                .first()
                .cloned()
                .unwrap_or_else(|| "所选技能当前不可用".to_string());
            steps.push(ExecutionStep::error(
                "加载技能提示",
                "所选技能当前不可用，请重新选择。",
            ));
            anyhow::bail!("所选技能当前不可用：{reason}");
        }
        if skill_runtime.catalog_count > 0 || !skill_runtime.activated.is_empty() {
            steps.push(ExecutionStep::done(
                "加载技能提示",
                if skill_runtime.activated.is_empty() {
                    "已注入技能目录（按需 use_skill）".to_string()
                } else {
                    format!("已激活技能 {}", skill_runtime.activated[0].skill_id)
                },
            ));
        }

        append_knowledge_prompt(&mut agent_cfg.system_prompt, request.workspace).await;
        append_research_policy(&mut agent_cfg.system_prompt, &tools);
        append_computer_use_policy(&mut agent_cfg.system_prompt, &tools);

        if let Some(chat_only_prompt) = request.security.chat_only_system_prompt() {
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{current}\n\n{chat_only_prompt}"));
            steps.push(ExecutionStep::done("飞书聊天模式", "已注入 chat_only 限制"));
        }

        let agent = Agent::new(
            provider,
            tools,
            memory,
            agent_cfg,
            request.security.clone(),
        );
        let browser_takeover = browser_takeover_slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        Ok(AssembledAgent {
            agent,
            browser_takeover,
        })
    }
}

/// Narrow the toolset to what a delegate agent is allowed to use. An empty
/// allowlist means "everything the registry built".
pub(super) fn apply_agent_tool_allowlist(
    cfg: &Config,
    agent_name: &str,
    tools: &mut Vec<Box<dyn Tool>>,
) {
    if cfg.permissions.is_full_access() {
        return;
    }
    let Some(delegate) = cfg.agents.get(agent_name) else {
        return;
    };
    if delegate.allowed_tools.is_empty() {
        return;
    }
    let allowed: HashSet<&str> = delegate.allowed_tools.iter().map(String::as_str).collect();
    tools.retain(|tool| allowed.contains(tool.name()));
}

/// Tell the agent where it is working. Without this the model guesses paths
/// like `/workspace` or `~` and then tries to probe them with shell calls.
fn append_workspace_note(
    system_prompt: &mut Option<String>,
    workspace: &Path,
    full_access: bool,
) {
    let path_rule = if full_access {
        "当前启用了完全访问模式。文件工具和 Shell working_directory 可以使用绝对路径或 Workspace 相对路径；操作仍受 OmniNova 进程自身的系统权限、输入校验和运行时完整性约束。"
    } else {
        "调用文件、搜索、Shell working_directory 或 Git 工具时，所有 path 必须是 workspace-relative；Workspace 根目录用 \".\"。不要把 D:\\、E:\\ 或完整 Workspace 绝对路径传给工具。写 index.html 时 path 只能是 \"index.html\"。"
    };
    let note = format!(
        "\n[环境信息] 当前 Workspace 目录是：{}。回答“你当前 workspace 在哪里”这类问题时，必须直接引用本路径，不要尝试通过 shell 或 file_read 探测 /workspace、/home、~ 等路径。\n[工具路径规则] {path_rule} 编辑已存在文件时优先 file_read 后使用 file_patch；新建文件才使用 file_write，除非用户明确要求整文件重写。\n[Office 交付规则] 用户要求本地 Word、PowerPoint 或 Excel 文件时，先整理完整内容，再只调用一次 office_create 生成真实可编辑的 .docx、.pptx 或 .xlsx。不得手工拆包或反复修改 OOXML，不得启动 WPS/PowerPoint 做试错，不得用 HTML、Markdown、CSV、XML 或改扩展名冒充 Office 文件。制作 PPT 时把整套 slides 一次传入，保持低信息密度和精炼标题；有工作区 PNG/JPEG 素材时填写 image_path 和 image_alt，并按内容选 auto、image_left、image_right 或 full_bleed，必要时填写 accent_color，避免整套幻灯片只有文字。制作公文时使用 document_style=official_cn，并分别填写 recipient、issuer、date。除非 office_create 明确返回失败，否则生成后直接交付，不追加 shell 验证。\n[KDocs 规则] use_skill 成功表示技能指令已加载到活动上下文；持久化回执会移除重复指令，绝不代表“空指令”。只有用户明确要求创建金山云文档时才走 KDocs；若新机器缺少 kdocs-cli 或尚未认证，要准确说明缺少的依赖或认证，不得声称技能内容为空。本地 Office 文件直接用 office_create，不依赖 KDocs/Python/Node。\n",
        workspace.display()
    );
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}{note}"));
}

fn append_research_policy(system_prompt: &mut Option<String>, tools: &[Box<dyn Tool>]) {
    let names: HashSet<&str> = tools.iter().map(|tool| tool.name()).collect();
    if !names.contains("web_search") && !names.contains("web_fetch") {
        return;
    }
    let mut note = String::from("\n[信息获取]\n");
    if names.contains("web_search") {
        note.push_str("- 新闻、政策、行情、百科、公开事实：必须先用 web_search。需要正文时设 read_top=2 或对结果 URL 使用 web_fetch。\n");
        note.push_str("- 回答时列出数据来源（站点名 + URL），数字注明日期。\n");
    } else {
        note.push_str("- 阅读公开网页用 web_fetch，不要为打开新闻站而启动 browser。\n");
    }
    if names.contains("browser") {
        note.push_str("- 禁止用 browser 搜索或打开普通新闻/文档页。browser 只用于登录、填表、验证码，或 web_fetch 明确失败（空页/403/JS 墙）。\n");
    }
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}{note}"));
}

fn append_computer_use_policy(system_prompt: &mut Option<String>, tools: &[Box<dyn Tool>]) {
    if !tools.iter().any(|tool| tool.name() == "computer_use") {
        return;
    }
    let note = "\n[桌面操作 computer_use]\n\
- 原生桌面应用（钉钉/飞书客户端/Excel/用友/金蝶等）用 computer_use；网页用 browser；公开检索用 web_search/web_fetch。\n\
- 先 snapshot 看控件，再 click name 或 ref（如 @e1）。坐标 x,y 只是后备，且必须相对最近一张截图像素，原点左上角。\n\
- 同一按钮连点无效时不要死磕：换 snapshot / 换 name。连续失败会被熔断。\n\
- type 通过剪贴板粘贴，适合中文。不要用 computer_use 去点浏览器窗口。\n\
- 长任务拆成多回合：todo_write 列步骤，task_checkpoint 记录进度（evidence 必须带截图路径）后结束本回合。\n\
- 禁止点击支付、关机、删除系统文件或密码框；这类情况必须停下来等人。\n";
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}{note}"));
}

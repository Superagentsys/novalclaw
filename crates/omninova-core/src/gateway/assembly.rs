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

/// Everything that varies between entry points. Anything not listed here is
/// derived from config so it stays identical across paths.
pub(crate) struct AgentAssemblyRequest<'a> {
    pub route: &'a RouteDecision,
    pub channel: &'a ChannelKind,
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
    ) -> anyhow::Result<Agent> {
        let agent_name = request.route.agent_name.as_str();

        let provider = build_provider_with_selection(
            cfg,
            &ProviderSelection {
                provider: request.route.provider.clone(),
                model: request.route.model.clone(),
            },
        );

        let memory = self.memory().await;
        let mut tools = build_tools(&ToolBuildContext {
            config: cfg,
            workspace: request.workspace,
            memory: Some(&memory),
            session_id: request.session_id,
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

        append_workspace_note(&mut agent_cfg.system_prompt, request.workspace);

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

        if let Some(chat_only_prompt) = request.security.chat_only_system_prompt() {
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{current}\n\n{chat_only_prompt}"));
            steps.push(ExecutionStep::done("飞书聊天模式", "已注入 chat_only 限制"));
        }

        Ok(Agent::new(
            provider,
            tools,
            memory,
            agent_cfg,
            request.security.clone(),
        ))
    }
}

/// Narrow the toolset to what a delegate agent is allowed to use. An empty
/// allowlist means "everything the registry built".
pub(super) fn apply_agent_tool_allowlist(
    cfg: &Config,
    agent_name: &str,
    tools: &mut Vec<Box<dyn Tool>>,
) {
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
fn append_workspace_note(system_prompt: &mut Option<String>, workspace: &Path) {
    let note = format!(
        "\n[环境信息] 当前 Workspace 目录是：{}。回答“你当前 workspace 在哪里”这类问题时，必须直接引用本路径，不要尝试通过 shell 或 file_read 探测 /workspace、/home、~ 等路径。\n[工具路径规则] 调用文件、搜索、Shell working_directory 或 Git 工具时，所有 path 必须是 workspace-relative；Workspace 根目录用 \".\"。不要把 D:\\、E:\\ 或完整 Workspace 绝对路径传给工具。写 index.html 时 path 只能是 \"index.html\"。编辑已存在文件时优先 file_read 后使用 file_patch；新建文件才使用 file_write，除非用户明确要求整文件重写。\n[Office 交付规则] 用户要求本地 Word、PowerPoint 或 Excel 文件时，先整理完整内容，再只调用一次 office_create 生成真实可编辑的 .docx、.pptx 或 .xlsx。不得手工拆包或反复修改 OOXML，不得启动 WPS/PowerPoint 做试错，不得用 HTML、Markdown、CSV、XML 或改扩展名冒充 Office 文件。制作 PPT 时把整套 slides 一次传入，保持低信息密度和精炼标题；有工作区 PNG/JPEG 素材时填写 image_path 和 image_alt，并按内容选 auto、image_left、image_right 或 full_bleed，必要时填写 accent_color，避免整套幻灯片只有文字。制作公文时使用 document_style=official_cn，并分别填写 recipient、issuer、date。除非 office_create 明确返回失败，否则生成后直接交付，不追加 shell 验证。\n[KDocs 规则] use_skill 成功表示技能指令已加载到活动上下文；持久化回执会移除重复指令，绝不代表“空指令”。只有用户明确要求创建金山云文档时才走 KDocs；若新机器缺少 kdocs-cli 或尚未认证，要准确说明缺少的依赖或认证，不得声称技能内容为空。本地 Office 文件直接用 office_create，不依赖 KDocs/Python/Node。\n",
        workspace.display()
    );
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}{note}"));
}

use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// 一次「向用户提问并等待」的交互请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPrompt {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub allow_free_text: bool,
    /// 触发提问时所基于的中间结果/上下文摘要（可选），便于用户判断。
    #[serde(default)]
    pub context: Option<String>,
}

/// 本轮执行期间收集到的待回答提问；由 gateway 在回合结束后读取并回传 UI。
pub type UserPromptSink = Arc<Mutex<Vec<UserPrompt>>>;

pub fn new_user_prompt_sink() -> UserPromptSink {
    Arc::new(Mutex::new(Vec::new()))
}

/// 让 Agent 在「不确定 / 需要根据中间结果让人来决定」时，主动向用户提问。
/// 调用后本回合暂停，问题与可选项回传到界面；用户的回答会作为下一条消息进入对话。
pub struct AskUserTool {
    sink: UserPromptSink,
}

impl AskUserTool {
    pub fn new(sink: UserPromptSink) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "当你对如何继续不确定、或需要根据已得到的中间结果让用户来决定方向时，调用本工具向用户提问。\
         适用场景：结果有歧义、有多种可行方案、涉及主观偏好、风险较高需用户拍板、或缺少关键信息。\
         调用后本回合会暂停等待用户回答；在收到回答前不要臆测答案或继续执行后续步骤。\
         尽量给出 2-5 个明确的候选项（options），用户也可自由作答。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "要向用户提出的问题，应清楚说明当前情况与需要决定什么"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "可选的候选答案（建议 2-5 个），用户点击即可选择"
                },
                "allow_free_text": {
                    "type": "boolean",
                    "description": "是否允许用户不选候选项、直接自由作答（默认 true）"
                },
                "context": {
                    "type": "string",
                    "description": "触发提问所依据的中间结果/上下文摘要（可选）"
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing 'question' parameter"))?
            .to_string();

        let options = args
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let allow_free_text = args
            .get("allow_free_text")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let context = args
            .get("context")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let prompt = UserPrompt {
            id: format!("ask-{}", Uuid::new_v4()),
            question: question.clone(),
            options: options.clone(),
            allow_free_text,
            context,
        };
        self.sink.lock().await.push(prompt);

        let options_hint = if options.is_empty() {
            String::new()
        } else {
            format!("（候选项：{}）", options.join(" / "))
        };
        Ok(ToolResult {
            success: true,
            output: format!(
                "已向用户提出问题并暂停等待回答{options_hint}：{question}。\
                 在用户回答之前，请勿臆测答案或继续后续步骤；请用一句话把问题清楚地呈现给用户。"
            ),
            error: None,
        })
    }
}

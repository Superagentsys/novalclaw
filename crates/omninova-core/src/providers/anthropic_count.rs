//! Shared Anthropic-native token-count request conversion and transport.
//!
//! The normal Anthropic chat adapter is currently OpenAI-compatible. A native
//! count result is therefore display-exact only when every model-visible
//! logical item can be represented without omission in the Anthropic request.

use crate::providers::model_capabilities::TokenMeasurement;
use crate::providers::{ChatMessage, ToolCall};
use crate::tools::ToolSpec;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct AnthropicCountConfig {
    pub base_url: String,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnthropicCountUnsupported {
    Images,
    Role(String),
    AssistantToolCallShape,
    AssistantReasoningContent,
    ToolResultShape,
    ToolArguments,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnthropicCountRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub system: Vec<AnthropicTextBlock>,
    pub messages: Vec<AnthropicCountMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AnthropicCountTool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnthropicTextBlock {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnthropicCountMessage {
    role: &'static str,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnthropicCountTool {
    name: String,
    description: String,
    input_schema: Value,
}

/// Converts the exact model-facing logical messages into Anthropic-native
/// count input. Unsupported content returns an error rather than being dropped.
pub(crate) fn build_anthropic_count_request(
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> Result<AnthropicCountRequest, AnthropicCountUnsupported> {
    let mut system = Vec::new();
    let mut native_messages = Vec::new();

    for message in messages {
        if message.images.as_ref().is_some_and(|images| !images.is_empty()) {
            return Err(AnthropicCountUnsupported::Images);
        }

        match message.role.as_str() {
            "system" => system.push(AnthropicTextBlock {
                kind: "text",
                text: message.content.clone(),
            }),
            "user" => native_messages.push(AnthropicCountMessage {
                role: "user",
                content: vec![AnthropicContentBlock::Text {
                    text: message.content.clone(),
                }],
            }),
            "assistant" => native_messages.push(convert_assistant_message(message)?),
            "tool" => native_messages.push(convert_tool_result(message)?),
            role => return Err(AnthropicCountUnsupported::Role(role.to_string())),
        }
    }

    Ok(AnthropicCountRequest {
        model: model.to_string(),
        system,
        messages: native_messages,
        tools: tools
            .iter()
            .map(|tool| AnthropicCountTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.parameters.clone(),
            })
            .collect(),
    })
}

fn convert_assistant_message(
    message: &ChatMessage,
) -> Result<AnthropicCountMessage, AnthropicCountUnsupported> {
    let parsed = serde_json::from_str::<Value>(&message.content).ok();
    let has_tool_calls = parsed
        .as_ref()
        .and_then(|value| value.get("tool_calls"))
        .is_some();
    if !has_tool_calls {
        return Ok(AnthropicCountMessage {
            role: "assistant",
            content: vec![AnthropicContentBlock::Text {
                text: message.content.clone(),
            }],
        });
    }

    let value = parsed.ok_or(AnthropicCountUnsupported::AssistantToolCallShape)?;
    if value
        .get("reasoning_content")
        .and_then(Value::as_str)
        .is_some_and(|reasoning| !reasoning.is_empty())
    {
        return Err(AnthropicCountUnsupported::AssistantReasoningContent);
    }
    let calls = serde_json::from_value::<Vec<ToolCall>>(
        value
            .get("tool_calls")
            .cloned()
            .ok_or(AnthropicCountUnsupported::AssistantToolCallShape)?,
    )
    .map_err(|_| AnthropicCountUnsupported::AssistantToolCallShape)?;
    if calls.is_empty() {
        return Err(AnthropicCountUnsupported::AssistantToolCallShape);
    }

    let mut content = Vec::new();
    if let Some(text) = value.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(AnthropicContentBlock::Text {
                text: text.to_string(),
            });
        }
    }
    for call in calls {
        let input = serde_json::from_str::<Value>(&call.arguments)
            .map_err(|_| AnthropicCountUnsupported::ToolArguments)?;
        if !input.is_object() {
            return Err(AnthropicCountUnsupported::ToolArguments);
        }
        content.push(AnthropicContentBlock::ToolUse {
            id: call.id,
            name: call.name,
            input,
        });
    }
    Ok(AnthropicCountMessage {
        role: "assistant",
        content,
    })
}

fn convert_tool_result(
    message: &ChatMessage,
) -> Result<AnthropicCountMessage, AnthropicCountUnsupported> {
    let value = serde_json::from_str::<Value>(&message.content)
        .map_err(|_| AnthropicCountUnsupported::ToolResultShape)?;
    let tool_use_id = value
        .get("tool_call_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(AnthropicCountUnsupported::ToolResultShape)?;
    let visible_content = value
        .get("content")
        .and_then(Value::as_str)
        .ok_or(AnthropicCountUnsupported::ToolResultShape)?;
    Ok(AnthropicCountMessage {
        role: "user",
        content: vec![AnthropicContentBlock::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: visible_content.to_string(),
        }],
    })
}

/// Authoritative Anthropic count transport. All callers use the same request
/// conversion and safe failure contract.
pub(crate) async fn count_anthropic_tokens(
    config: &AnthropicCountConfig,
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    timeout: Duration,
) -> Option<TokenMeasurement> {
    let request_body = build_anthropic_count_request(model, messages, tools).ok()?;
    let client = reqwest::Client::builder()
        .connect_timeout(timeout)
        .build()
        .ok()?;
    let mut request = client
        .post(format!(
            "{}/v1/messages/count_tokens",
            config.base_url.trim_end_matches('/')
        ))
        .json(&request_body);
    if let Some(credential) = &config.credential {
        request = request
            .header("x-api-key", credential)
            .header("anthropic-version", "2023-06-01");
    }
    let response = tokio::time::timeout(timeout, request.send())
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let value: Value = response.json().await.ok()?;
    Some(TokenMeasurement {
        tokens: value.get("input_tokens")?.as_u64()?,
        source: "provider_count_api",
        canonical_model: Some(model.to_string()),
        exact: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool() -> ToolSpec {
        ToolSpec {
            name: "file_read".into(),
            description: "Read a file".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"}}}),
        }
    }

    #[test]
    fn logical_request_contains_system_text_tools_tool_use_and_visible_result_only() {
        let mut result = ChatMessage::tool(
            json!({"tool_call_id":"call-1","content":"VISIBLE_RESULT"}).to_string(),
        );
        result.original_tool_content = Some("HIDDEN_ORIGINAL_RESULT".into());
        let messages = vec![
            ChatMessage::system("SYSTEM_RULES"),
            ChatMessage::user("USER_TEXT"),
            ChatMessage::assistant("ASSISTANT_TEXT"),
            ChatMessage::assistant(
                json!({
                    "content":"TOOL_PREFACE",
                    "tool_calls":[{"id":"call-1","name":"file_read","arguments":"{\"path\":\"index.html\"}"}]
                })
                .to_string(),
            ),
            result,
        ];
        let value = serde_json::to_value(
            build_anthropic_count_request("claude-sonnet-4-5", &messages, &[tool()]).unwrap(),
        )
        .unwrap();
        let serialized = value.to_string();
        assert!(serialized.contains("SYSTEM_RULES"));
        assert!(serialized.contains("USER_TEXT"));
        assert!(serialized.contains("ASSISTANT_TEXT"));
        assert!(serialized.contains("file_read"));
        assert!(serialized.contains("tool_use"));
        assert!(serialized.contains("VISIBLE_RESULT"));
        assert!(!serialized.contains("HIDDEN_ORIGINAL_RESULT"));
        assert!(value.get("tools").and_then(Value::as_array).is_some_and(|v| v.len() == 1));
    }

    #[test]
    fn unsupported_content_is_rejected_instead_of_silently_dropped() {
        let image = ChatMessage::user_with_images("look", vec!["data:image/png;base64,AAAA".into()]);
        assert!(matches!(
            build_anthropic_count_request("claude-sonnet-4-5", &[image], &[]),
            Err(AnthropicCountUnsupported::Images)
        ));

        let malformed_tool = ChatMessage::tool("not-json");
        assert!(matches!(
            build_anthropic_count_request("claude-sonnet-4-5", &[malformed_tool], &[]),
            Err(AnthropicCountUnsupported::ToolResultShape)
        ));
    }
}

//! OpenAI-compatible Provider-native request shapes.
//!
//! Context usage classification uses these same structures so breakdowns are
//! derived from the serialized request the Provider actually sends, not from a
//! parallel ChatMessage reconstruction.

use crate::providers::traits::{ChatMessage, ToolCall};
use crate::tools::ToolSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct NativeChatRequest {
    pub model: String,
    pub messages: Vec<NativeMessage>,
    pub temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<NativeToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct NativeMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<NativeToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct NativeToolSpec {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: NativeToolFunctionSpec,
}

#[derive(Debug, Serialize, Clone)]
pub struct NativeToolFunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NativeToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub function: NativeFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NativeFunctionCall {
    pub name: String,
    pub arguments: String,
}

pub fn convert_tools(tools: Option<&[ToolSpec]>) -> Option<Vec<NativeToolSpec>> {
    tools.filter(|items| !items.is_empty()).map(|items| {
        items
            .iter()
            .map(|tool| NativeToolSpec {
                kind: "function".to_string(),
                function: NativeToolFunctionSpec {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                },
            })
            .collect()
    })
}

pub fn convert_messages(messages: &[ChatMessage]) -> Vec<NativeMessage> {
    messages
        .iter()
        .filter_map(|m| {
            if m.role == "assistant" {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                    if let Some(tool_calls_value) = value.get("tool_calls") {
                        if let Ok(parsed_calls) =
                            serde_json::from_value::<Vec<ToolCall>>(tool_calls_value.clone())
                        {
                            if !parsed_calls.is_empty() {
                                let tool_calls = parsed_calls
                                    .into_iter()
                                    .map(|tc| NativeToolCall {
                                        id: Some(tc.id),
                                        kind: Some("function".to_string()),
                                        function: NativeFunctionCall {
                                            name: tc.name,
                                            arguments: tc.arguments,
                                        },
                                    })
                                    .collect::<Vec<_>>();
                                let content = value
                                    .get("content")
                                    .and_then(serde_json::Value::as_str)
                                    .map(ToString::to_string);
                                let reasoning_content = value
                                    .get("reasoning_content")
                                    .and_then(serde_json::Value::as_str)
                                    .map(ToString::to_string);
                                return Some(NativeMessage {
                                    role: "assistant".to_string(),
                                    content: content.map(serde_json::Value::String),
                                    tool_call_id: None,
                                    tool_calls: Some(tool_calls),
                                    reasoning_content,
                                });
                            }
                        }
                    }
                }
            }

            if m.role == "tool" {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                    let tool_call_id = value
                        .get("tool_call_id")
                        .and_then(serde_json::Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(ToString::to_string);
                    let content = value
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string);
                    if tool_call_id.is_some() {
                        return Some(NativeMessage {
                            role: "tool".to_string(),
                            content: content.map(serde_json::Value::String),
                            tool_call_id,
                            tool_calls: None,
                            reasoning_content: None,
                        });
                    }
                }
                return None;
            }

            let content = if m.role == "user" {
                Some(user_content_value(m))
            } else {
                Some(serde_json::Value::String(m.content.clone()))
            };

            Some(NativeMessage {
                role: m.role.clone(),
                content,
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            })
        })
        .collect()
}

fn user_content_value(message: &ChatMessage) -> serde_json::Value {
    let images = message.images.as_deref().unwrap_or_default();
    if images.is_empty() {
        return serde_json::Value::String(message.content.clone());
    }

    let mut parts = vec![serde_json::json!({
        "type": "text",
        "text": message.content,
    })];
    for url in images {
        parts.push(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": url },
        }));
    }
    serde_json::Value::Array(parts)
}

/// Candidate-facing native view: the same messages/tools objects the Provider
/// would send, without the final envelope fields (model/stream/tool_choice).
pub fn native_context_view_json(messages: &[ChatMessage], tools: &[ToolSpec]) -> String {
    #[derive(Serialize)]
    struct View {
        messages: Vec<NativeMessage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tools: Option<Vec<NativeToolSpec>>,
    }
    serde_json::to_string(&View {
        messages: convert_messages(messages),
        tools: convert_tools(if tools.is_empty() {
            None
        } else {
            Some(tools)
        }),
    })
    .unwrap_or_default()
}

/// Full OpenAI-compatible envelope used at send time.
pub fn serialize_native_chat_request(request: &NativeChatRequest) -> anyhow::Result<String> {
    serde_json::to_string(request).map_err(|error| anyhow::anyhow!(error))
}

impl NativeChatRequest {
    /// Semantic output cap for this finalized OpenAI-compatible envelope.
    pub fn output_limit_tokens(&self) -> Option<u64> {
        self.max_tokens
            .map(u64::from)
            .filter(|tokens| *tokens > 0)
    }
}

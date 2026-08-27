//! Official DeepSeek V4 Flash prompt encoding and pinned local tokenizer.
//!
//! Display metering only. C1/C2 keep using [`TokenEstimator`].
//! Production parity is not claimed until V1.2B live comparison succeeds.

use crate::providers::model_capabilities::TokenMeasurement;
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokenizers::Tokenizer;

pub const TOKENIZER_NAME: &str = "deepseek_v4_flash_0731";
pub const CANONICAL_MODEL: &str = "deepseek-v4-flash";
pub const TOKENIZER_FAMILY: &str = "deepseek_v4";
pub const TOKENIZER_REVISION: &str = "DeepSeek-V4-Flash-0731";
const _PINNED_TOKENIZER: (&str, &str, &str) =
    (CANONICAL_MODEL, TOKENIZER_FAMILY, TOKENIZER_REVISION);

const BOS_TOKEN: &str = "<｜begin▁of▁sentence｜>";
const EOS_TOKEN: &str = "<｜end▁of▁sentence｜>";
const THINKING_START_TOKEN: &str = "<think>";
const THINKING_END_TOKEN: &str = "</think>";
const DSML_TOKEN: &str = "｜DSML｜";
const USER_SP_TOKEN: &str = "<｜User｜>";
const ASSISTANT_SP_TOKEN: &str = "<｜Assistant｜>";
const LATEST_REMINDER_SP_TOKEN: &str = "<｜latest_reminder｜>";
const TOOL_CALLS_BLOCK_NAME: &str = "tool_calls";
const DEFAULT_REASONING_EFFORT: &str = "low";

const TOKENIZER_JSON: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/deepseek-v4-flash-0731/tokenizer.json"
));

const TOOLS_TEMPLATE: &str = "\
## Tools\n\
\n\
You have access to a set of tools to help answer the user's question. You can invoke tools by writing a \"<{dsml_token}tool_calls>\" block like the following:\n\
\n\
<{dsml_token}tool_calls>\n\
<{dsml_token}invoke name=\"$TOOL_NAME\">\n\
<{dsml_token}parameter name=\"$PARAMETER_NAME\" string=\"true|false\">$PARAMETER_VALUE</{dsml_token}parameter>\n\
...\n\
</{dsml_token}invoke>\n\
<{dsml_token}invoke name=\"$TOOL_NAME2\">\n\
...\n\
</{dsml_token}invoke>\n\
</{dsml_token}tool_calls>\n\
\n\
String parameters should be specified as is and set `string=\"true\"`. For all other types (numbers, booleans, arrays, objects), pass the value in JSON format and set `string=\"false\"`.\n\
\n\
If thinking_mode is enabled (triggered by {thinking_start_token}), you MUST output your complete reasoning inside {thinking_start_token}...{thinking_end_token} BEFORE any tool calls or final response.\n\
\n\
Otherwise, output directly after {thinking_end_token} with tool calls or final response.\n\
\n\
### Available Tool Schemas\n\
\n\
{tool_schemas}\n\
\n\
You MUST strictly follow the above defined tool name and parameter schemas to invoke tool calls.\n";

const REASONING_EFFORT_HIGH: &str = "\
Reasoning Effort: Absolute maximum with no shortcuts permitted.\n\
You MUST be very thorough in your thinking and comprehensively decompose the problem to resolve the root cause, rigorously stress-testing your logic against all potential paths, edge cases, and adversarial scenarios.\n\
Explicitly write out your entire deliberation process, documenting every intermediate step, considered alternative, and rejected hypothesis to ensure absolutely no assumption is left unchecked.\n\
\n";

const REASONING_EFFORT_MAX: &str = "\
Reasoning Effort: Beyond maximum — exhaustive, relentless, and uncompromising.\n\
You MUST reason with the utmost depth and rigor, leaving absolutely nothing to chance: exhaustively decompose the problem into its most fundamental components, trace every causal chain to its root, and resolve the underlying cause rather than any surface symptom.\n\
Do not stop reasoning until you have independently verified the solution from multiple angles and are certain that no assumption remains unchecked and no error remains undiscovered.\n\
\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    Chat,
    Thinking,
}

impl ThinkingMode {
    #[allow(dead_code)]
    fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Thinking => "thinking",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeepSeekV4RequestSettings {
    pub thinking_mode: ThinkingMode,
    pub reasoning_effort: ReasoningEffort,
}

impl Default for ThinkingMode {
    fn default() -> Self {
        Self::Chat
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningEffort {
    #[default]
    Low,
    High,
    Max,
}

impl ReasoningEffort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    #[allow(dead_code)]
    fn prefix(self) -> &'static str {
        match self {
            Self::Low => "",
            Self::High => REASONING_EFFORT_HIGH,
            Self::Max => REASONING_EFFORT_MAX,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepSeekV4Unsupported {
    Images,
    Role(String),
    AssistantToolCallShape,
    ToolResultShape,
    ToolArguments,
}

/// Parses thinking/reasoning settings from the serialized Provider request.
/// Absent fields default to chat / low so the counter does not assume thinking.
pub fn settings_from_request_body(request_body: Option<&str>) -> DeepSeekV4RequestSettings {
    let Some(body) = request_body.and_then(|raw| serde_json::from_str::<Value>(raw).ok()) else {
        return DeepSeekV4RequestSettings::default();
    };
    let kwargs = body
        .get("extra_body")
        .and_then(|v| v.get("chat_template_kwargs"));
    let thinking_source = kwargs
        .and_then(|v| v.get("thinking"))
        .or_else(|| body.get("thinking"));
    let mode_source = kwargs
        .and_then(|v| v.get("thinking_mode"))
        .or_else(|| body.get("thinking_mode"));
    let effort_source = kwargs
        .and_then(|v| v.get("reasoning_effort"))
        .or_else(|| body.get("reasoning_effort"));

    let thinking_mode = if let Some(mode) = mode_source.and_then(Value::as_str) {
        match mode {
            "thinking" => ThinkingMode::Thinking,
            _ => ThinkingMode::Chat,
        }
    } else if thinking_source.and_then(Value::as_bool) == Some(true)
        || thinking_source.and_then(Value::as_str) == Some("true")
        || thinking_source.and_then(Value::as_str) == Some("thinking")
    {
        ThinkingMode::Thinking
    } else {
        ThinkingMode::Chat
    };

    let reasoning_effort = match effort_source.and_then(Value::as_str).unwrap_or(DEFAULT_REASONING_EFFORT)
    {
        "high" => ReasoningEffort::High,
        "max" => ReasoningEffort::Max,
        _ => ReasoningEffort::Low,
    };
    DeepSeekV4RequestSettings {
        thinking_mode,
        reasoning_effort,
    }
}

pub fn count_deepseek_v4_flash_tokens(
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    settings: DeepSeekV4RequestSettings,
) -> Result<TokenMeasurement, DeepSeekV4Unsupported> {
    let encoded = encode_omninova_messages(messages, tools, settings)?;
    let tokenizer = load_tokenizer().map_err(|_| DeepSeekV4Unsupported::AssistantToolCallShape)?;
    let encoding = tokenizer
        .encode(encoded, false)
        .map_err(|_| DeepSeekV4Unsupported::AssistantToolCallShape)?;
    Ok(TokenMeasurement {
        tokens: encoding.get_ids().len() as u64,
        source: "exact_local_tokenizer",
        canonical_model: Some(CANONICAL_MODEL.to_string()),
        exact: false,
    })
}

pub fn encode_omninova_messages(
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    settings: DeepSeekV4RequestSettings,
) -> Result<String, DeepSeekV4Unsupported> {
    let mut payload = omninova_to_v4_messages(messages, tools)?;
    encode_messages(
        &mut payload,
        settings.thinking_mode,
        true,
        settings.reasoning_effort.as_str(),
    )
    .map_err(|_| DeepSeekV4Unsupported::AssistantToolCallShape)
}

fn omninova_to_v4_messages(
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> Result<Vec<Value>, DeepSeekV4Unsupported> {
    let openai_tools = if tools.is_empty() {
        None
    } else {
        Some(Value::Array(
            tools
                .iter()
                .map(|tool| {
                    json_object([
                        ("type", Value::String("function".into())),
                        (
                            "function",
                            json_object([
                                ("name", Value::String(tool.name.clone())),
                                ("description", Value::String(tool.description.clone())),
                                ("parameters", tool.parameters.clone()),
                            ]),
                        ),
                    ])
                })
                .collect(),
        ))
    };

    let mut out = Vec::new();
    let mut attached_tools = false;
    for message in messages {
        if message.images.as_ref().is_some_and(|images| !images.is_empty()) {
            return Err(DeepSeekV4Unsupported::Images);
        }
        match message.role.as_str() {
            "system" => {
                let mut obj = json_object([
                    ("role", Value::String("system".into())),
                    ("content", Value::String(message.content.clone())),
                ]);
                if !attached_tools {
                    if let Some(tools) = &openai_tools {
                        obj.as_object_mut()
                            .expect("system object")
                            .insert("tools".into(), tools.clone());
                        attached_tools = true;
                    }
                }
                out.push(obj);
            }
            "user" => out.push(json_object([
                ("role", Value::String("user".into())),
                ("content", Value::String(message.content.clone())),
            ])),
            "assistant" => out.push(convert_assistant(message)?),
            "tool" => out.push(convert_tool(message)?),
            role => return Err(DeepSeekV4Unsupported::Role(role.to_string())),
        }
    }
    if let Some(tools) = openai_tools {
        if !attached_tools {
            let mut system = json_object([
                ("role", Value::String("system".into())),
                ("content", Value::String(String::new())),
            ]);
            system
                .as_object_mut()
                .expect("system object")
                .insert("tools".into(), tools);
            out.insert(0, system);
        }
    }
    Ok(out)
}

fn convert_assistant(message: &ChatMessage) -> Result<Value, DeepSeekV4Unsupported> {
    let parsed = serde_json::from_str::<Value>(&message.content).ok();
    let has_tool_calls = parsed
        .as_ref()
        .and_then(|value| value.get("tool_calls"))
        .is_some();
    if !has_tool_calls {
        return Ok(json_object([
            ("role", Value::String("assistant".into())),
            ("content", Value::String(message.content.clone())),
        ]));
    }
    let value = parsed.ok_or(DeepSeekV4Unsupported::AssistantToolCallShape)?;
    let calls = value
        .get("tool_calls")
        .cloned()
        .ok_or(DeepSeekV4Unsupported::AssistantToolCallShape)?;
    let calls_arr = calls
        .as_array()
        .ok_or(DeepSeekV4Unsupported::AssistantToolCallShape)?;
    if calls_arr.is_empty() {
        return Err(DeepSeekV4Unsupported::AssistantToolCallShape);
    }
    let mut openai_calls = Vec::new();
    for call in calls_arr {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .ok_or(DeepSeekV4Unsupported::AssistantToolCallShape)?;
        let arguments = call
            .get("arguments")
            .and_then(Value::as_str)
            .ok_or(DeepSeekV4Unsupported::AssistantToolCallShape)?;
        let parsed_args: Value = serde_json::from_str(arguments)
            .map_err(|_| DeepSeekV4Unsupported::ToolArguments)?;
        if !parsed_args.is_object() {
            return Err(DeepSeekV4Unsupported::ToolArguments);
        }
        openai_calls.push(json_object([
            ("id", Value::String(id)),
            ("type", Value::String("function".into())),
            (
                "function",
                json_object([
                    ("name", Value::String(name.to_string())),
                    ("arguments", Value::String(arguments.to_string())),
                ]),
            ),
        ]));
    }
    let mut obj = Map::new();
    obj.insert("role".into(), Value::String("assistant".into()));
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        obj.insert("content".into(), Value::String(content.to_string()));
    }
    if let Some(reasoning) = value.get("reasoning_content").and_then(Value::as_str) {
        if !reasoning.is_empty() {
            obj.insert(
                "reasoning_content".into(),
                Value::String(reasoning.to_string()),
            );
        }
    }
    obj.insert("tool_calls".into(), Value::Array(openai_calls));
    Ok(Value::Object(obj))
}

fn convert_tool(message: &ChatMessage) -> Result<Value, DeepSeekV4Unsupported> {
    let value = serde_json::from_str::<Value>(&message.content)
        .map_err(|_| DeepSeekV4Unsupported::ToolResultShape)?;
    let tool_call_id = value
        .get("tool_call_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(DeepSeekV4Unsupported::ToolResultShape)?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .ok_or(DeepSeekV4Unsupported::ToolResultShape)?;
    Ok(json_object([
        ("role", Value::String("tool".into())),
        ("tool_call_id", Value::String(tool_call_id.to_string())),
        ("content", Value::String(content.to_string())),
    ]))
}

fn json_object(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

fn load_tokenizer() -> Result<&'static Tokenizer, String> {
    static TOKENIZER: OnceLock<Result<Tokenizer, String>> = OnceLock::new();
    TOKENIZER
        .get_or_init(|| {
            Tokenizer::from_bytes(TOKENIZER_JSON).map_err(|err| err.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Faithful port of official `encode_messages`.
pub fn encode_messages(
    messages: &mut [Value],
    thinking_mode: ThinkingMode,
    add_default_bos_token: bool,
    reasoning_effort: &str,
) -> Result<String, String> {
    let mut owned: Vec<Value> = messages.iter().cloned().collect();
    owned = merge_tool_messages(owned);
    owned = sort_tool_results_by_call_order(owned);

    let mut effective_drop_thinking = true;
    if owned.iter().any(|msg| msg.get("tools").is_some()) {
        effective_drop_thinking = false;
    }

    if thinking_mode == ThinkingMode::Thinking && effective_drop_thinking {
        owned = drop_thinking_messages(owned);
    }

    let mut prompt = if add_default_bos_token {
        BOS_TOKEN.to_string()
    } else {
        String::new()
    };
    for idx in 0..owned.len() {
        prompt.push_str(&render_message(
            idx,
            &owned,
            thinking_mode,
            effective_drop_thinking,
            reasoning_effort,
        )?);
    }
    Ok(prompt)
}

fn reasoning_effort_prefix(effort: &str) -> Result<&'static str, String> {
    match effort {
        "low" => Ok(""),
        "high" => Ok(REASONING_EFFORT_HIGH),
        "max" => Ok(REASONING_EFFORT_MAX),
        other => Err(format!("Invalid reasoning effort: {other}")),
    }
}

fn find_last_user_index(messages: &[Value]) -> i32 {
    for idx in (0..messages.len()).rev() {
        if let Some("user" | "developer") = messages[idx].get("role").and_then(Value::as_str) {
            return idx as i32;
        }
    }
    -1
}

fn python_dumps(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", escape_json_string(s)),
        Value::Array(items) => {
            let inner = items
                .iter()
                .map(python_dumps)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        Value::Object(map) => {
            let inner = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", escape_json_string(k), python_dumps(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{inner}}}")
        }
    }
}

fn escape_json_string(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

fn tools_from_openai_format(tools: &Value) -> Option<Vec<Value>> {
    tools.as_array().map(|items| {
        items
            .iter()
            .filter_map(|tool| tool.get("function").cloned())
            .collect()
    })
}

fn encode_arguments_to_dsml(tool_call: &Value) -> String {
    let raw_arguments = tool_call
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = serde_json::from_str::<Value>(raw_arguments).unwrap_or_else(|_| {
        json_object([("arguments", Value::String(raw_arguments.to_string()))])
    });
    let Some(map) = arguments.as_object() else {
        return String::new();
    };
    let mut parts = Vec::new();
    for (key, value) in map {
        let is_str = value.is_string();
        let rendered = if let Some(text) = value.as_str() {
            text.to_string()
        } else {
            python_dumps(value)
        };
        parts.push(format!(
            "<{DSML_TOKEN}parameter name=\"{key}\" string=\"{}\">{rendered}</{DSML_TOKEN}parameter>",
            if is_str { "true" } else { "false" }
        ));
    }
    parts.join("\n")
}

fn render_tools(tools: &[Value]) -> String {
    let schemas = tools
        .iter()
        .map(python_dumps)
        .collect::<Vec<_>>()
        .join("\n");
    TOOLS_TEMPLATE
        .replace("{dsml_token}", DSML_TOKEN)
        .replace("{thinking_start_token}", THINKING_START_TOKEN)
        .replace("{thinking_end_token}", THINKING_END_TOKEN)
        .replace("{tool_schemas}", &schemas)
}

fn render_message(
    index: usize,
    messages: &[Value],
    thinking_mode: ThinkingMode,
    drop_thinking: bool,
    reasoning_effort: &str,
) -> Result<String, String> {
    let msg = &messages[index];
    let last_user_idx = find_last_user_index(messages);
    let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
    let content = msg.get("content").and_then(Value::as_str);
    let mut prompt = String::new();

    if index == 0 && thinking_mode == ThinkingMode::Thinking {
        prompt.push_str(reasoning_effort_prefix(reasoning_effort)?);
    }

    match role {
        "system" => {
            prompt.push_str(content.unwrap_or(""));
            if let Some(tools) = msg.get("tools").and_then(tools_from_openai_format) {
                prompt.push_str("\n\n");
                prompt.push_str(&render_tools(&tools));
            }
            if let Some(response_format) = msg.get("response_format") {
                prompt.push_str("\n\n## Response Format:\n\nYou MUST strictly adhere to the following schema to reply:\n");
                prompt.push_str(&python_dumps(response_format));
            }
        }
        "developer" => {
            let mut content_developer = USER_SP_TOKEN.to_string();
            content_developer.push_str(content.ok_or("Invalid developer message")?);
            if let Some(tools) = msg.get("tools").and_then(tools_from_openai_format) {
                content_developer.push_str("\n\n");
                content_developer.push_str(&render_tools(&tools));
            }
            prompt.push_str(&content_developer);
        }
        "user" => {
            prompt.push_str(USER_SP_TOKEN);
            if let Some(blocks) = msg.get("content_blocks").and_then(Value::as_array) {
                let mut parts = Vec::new();
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            parts.push(block.get("text").and_then(Value::as_str).unwrap_or("").to_string());
                        }
                        Some("tool_result") => {
                            let tool_content = match &block["content"] {
                                Value::String(text) => text.clone(),
                                Value::Array(items) => items
                                    .iter()
                                    .map(|item| {
                                        if item.get("type").and_then(Value::as_str) == Some("text") {
                                            item.get("text").and_then(Value::as_str).unwrap_or("").to_string()
                                        } else {
                                            format!(
                                                "[Unsupported {}]",
                                                item.get("type").and_then(Value::as_str).unwrap_or("unknown")
                                            )
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n"),
                                other => python_dumps(other),
                            };
                            parts.push(format!("<tool_result>{tool_content}</tool_result>"));
                        }
                        other => parts.push(format!(
                            "[Unsupported {}]",
                            other.unwrap_or("unknown")
                        )),
                    }
                }
                prompt.push_str(&parts.join("\n\n"));
            } else {
                prompt.push_str(content.unwrap_or(""));
            }
        }
        "latest_reminder" => {
            prompt.push_str(LATEST_REMINDER_SP_TOKEN);
            prompt.push_str(content.unwrap_or(""));
        }
        "tool" => {
            return Err("deepseek_v4 merges tool messages into user".into());
        }
        "assistant" => {
            let mut tc_content = String::new();
            if let Some(tool_calls) = msg.get("tool_calls").and_then(Value::as_array) {
                if !tool_calls.is_empty() {
                    let converted: Vec<Value> = tool_calls
                        .iter()
                        .map(|call| {
                            if call.get("function").is_some() {
                                json_object([
                                    (
                                        "name",
                                        Value::String(
                                            call.get("function")
                                                .and_then(|f| f.get("name"))
                                                .and_then(Value::as_str)
                                                .unwrap_or("")
                                                .to_string(),
                                        ),
                                    ),
                                    (
                                        "arguments",
                                        Value::String(
                                            call.get("function")
                                                .and_then(|f| f.get("arguments"))
                                                .and_then(Value::as_str)
                                                .unwrap_or("")
                                                .to_string(),
                                        ),
                                    ),
                                ])
                            } else {
                                call.clone()
                            }
                        })
                        .collect();
                    let tc_list: Vec<String> = converted
                        .iter()
                        .map(|tc| {
                            let name = tc.get("name").and_then(Value::as_str).unwrap_or("");
                            let arguments = encode_arguments_to_dsml(tc);
                            format!(
                                "<{DSML_TOKEN}invoke name=\"{name}\">\n{arguments}\n</{DSML_TOKEN}invoke>"
                            )
                        })
                        .collect();
                    tc_content.push_str("\n\n");
                    tc_content.push_str(&format!(
                        "<{DSML_TOKEN}{TOOL_CALLS_BLOCK_NAME}>\n{}\n</{DSML_TOKEN}{TOOL_CALLS_BLOCK_NAME}>",
                        tc_list.join("\n")
                    ));
                }
            }
            let summary_content = content.unwrap_or("");
            let rc = msg
                .get("reasoning_content")
                .and_then(Value::as_str)
                .unwrap_or("");
            let prev_has_task = index
                .checked_sub(1)
                .and_then(|prev| messages.get(prev))
                .and_then(|prev| prev.get("task"))
                .is_some();
            let thinking_part = if thinking_mode == ThinkingMode::Thinking && !prev_has_task {
                if !drop_thinking || (index as i32) > last_user_idx {
                    format!("{rc}{THINKING_END_TOKEN}")
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            let wo_eos = msg.get("wo_eos").and_then(Value::as_bool).unwrap_or(false);
            prompt.push_str(&thinking_part);
            prompt.push_str(summary_content);
            prompt.push_str(&tc_content);
            if !wo_eos {
                prompt.push_str(EOS_TOKEN);
            }
        }
        other => return Err(format!("Unknown role: {other}")),
    }

    if index + 1 < messages.len() {
        let next_role = messages[index + 1]
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("");
        if next_role != "assistant" && next_role != "latest_reminder" {
            return Ok(prompt);
        }
    }

    if let Some(task) = msg.get("task").and_then(Value::as_str) {
        let token = match task {
            "action" => "<｜action｜>",
            "query" => "<｜query｜>",
            "authority" => "<｜authority｜>",
            "domain" => "<｜domain｜>",
            "title" => "<｜title｜>",
            "read_url" => "<｜read_url｜>",
            other => return Err(format!("Invalid task: {other}")),
        };
        if task != "action" {
            prompt.push_str(token);
        } else {
            prompt.push_str(ASSISTANT_SP_TOKEN);
            prompt.push_str(if thinking_mode == ThinkingMode::Thinking {
                THINKING_START_TOKEN
            } else {
                THINKING_END_TOKEN
            });
            prompt.push_str(token);
        }
    } else if matches!(role, "user" | "developer") {
        prompt.push_str(ASSISTANT_SP_TOKEN);
        if (!drop_thinking && thinking_mode == ThinkingMode::Thinking)
            || (drop_thinking
                && thinking_mode == ThinkingMode::Thinking
                && (index as i32) >= last_user_idx)
        {
            prompt.push_str(THINKING_START_TOKEN);
        } else {
            prompt.push_str(THINKING_END_TOKEN);
        }
    }
    Ok(prompt)
}

fn merge_tool_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for msg in messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "tool" {
            let tool_block = json_object([
                ("type", Value::String("tool_result".into())),
                (
                    "tool_use_id",
                    Value::String(
                        msg.get("tool_call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                ),
                (
                    "content",
                    Value::String(
                        msg.get("content")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                ),
            ]);
            if let Some(last) = merged.last_mut() {
                if last.get("role").and_then(Value::as_str) == Some("user")
                    && last.get("content_blocks").is_some()
                {
                    last.as_object_mut()
                        .unwrap()
                        .get_mut("content_blocks")
                        .unwrap()
                        .as_array_mut()
                        .unwrap()
                        .push(tool_block);
                    continue;
                }
            }
            merged.push(json_object([
                ("role", Value::String("user".into())),
                ("content_blocks", Value::Array(vec![tool_block])),
            ]));
        } else if role == "user" {
            let text_block = json_object([
                ("type", Value::String("text".into())),
                (
                    "text",
                    Value::String(
                        msg.get("content")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                ),
            ]);
            let can_merge = merged.last().is_some_and(|last| {
                last.get("role").and_then(Value::as_str) == Some("user")
                    && last.get("content_blocks").is_some()
                    && last.get("task").is_none()
            });
            if can_merge {
                merged
                    .last_mut()
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .get_mut("content_blocks")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .push(text_block);
            } else {
                let mut new_msg = json_object([
                    ("role", Value::String("user".into())),
                    (
                        "content",
                        Value::String(
                            msg.get("content")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ),
                    ),
                    ("content_blocks", Value::Array(vec![text_block])),
                ]);
                if let Some(obj) = new_msg.as_object_mut() {
                    for key in ["task", "wo_eos", "mask"] {
                        if let Some(value) = msg.get(key) {
                            obj.insert(key.to_string(), value.clone());
                        }
                    }
                }
                merged.push(new_msg);
            }
        } else {
            merged.push(msg);
        }
    }
    merged
}

fn sort_tool_results_by_call_order(mut messages: Vec<Value>) -> Vec<Value> {
    let mut last_tool_call_order: HashMap<String, usize> = HashMap::new();
    for msg in &mut messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "assistant" {
            if let Some(tool_calls) = msg.get("tool_calls").and_then(Value::as_array) {
                last_tool_call_order.clear();
                for (idx, tc) in tool_calls.iter().enumerate() {
                    let id = tc
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            tc.get("function")
                                .and_then(|f| f.get("id"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("");
                    if !id.is_empty() {
                        last_tool_call_order.insert(id.to_string(), idx);
                    }
                }
            }
        } else if role == "user" {
            let Some(blocks) = msg.get("content_blocks").and_then(Value::as_array).cloned() else {
                continue;
            };
            let tool_blocks: Vec<Value> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                .cloned()
                .collect();
            if tool_blocks.len() > 1 && !last_tool_call_order.is_empty() {
                let mut sorted_blocks = tool_blocks;
                sorted_blocks.sort_by_key(|block| {
                    last_tool_call_order
                        .get(block.get("tool_use_id").and_then(Value::as_str).unwrap_or(""))
                        .copied()
                        .unwrap_or(0)
                });
                let mut sorted_idx = 0;
                let mut new_blocks = Vec::new();
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        new_blocks.push(sorted_blocks[sorted_idx].clone());
                        sorted_idx += 1;
                    } else {
                        new_blocks.push(block);
                    }
                }
                msg.as_object_mut()
                    .unwrap()
                    .insert("content_blocks".into(), Value::Array(new_blocks));
            }
        }
    }
    messages
}

fn drop_thinking_messages(messages: Vec<Value>) -> Vec<Value> {
    let last_user_idx = find_last_user_index(&messages);
    let keep_roles = ["user", "system", "tool", "latest_reminder", "direct_search_results"];
    let mut result = Vec::new();
    for (idx, mut msg) in messages.into_iter().enumerate() {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
        if keep_roles.contains(&role) || (idx as i32) >= last_user_idx {
            result.push(msg);
        } else if role == "assistant" {
            if let Some(obj) = msg.as_object_mut() {
                obj.remove("reasoning_content");
            }
            result.push(msg);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::context_budget::TokenEstimator;
    use std::fs;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/deepseek-v4-flash-0731/encoding/tests")
    }

    fn read_fixture(name: &str) -> String {
        fs::read_to_string(fixtures_dir().join(name)).expect(name)
    }

    fn official_case(input_name: &str, output_name: &str, mode: ThinkingMode) {
        let input: Value = serde_json::from_str(&read_fixture(input_name)).unwrap();
        let mut messages = if input.is_array() {
            input.as_array().unwrap().clone()
        } else {
            let mut msgs = input
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap();
            if let Some(tools) = input.get("tools") {
                msgs[0]
                    .as_object_mut()
                    .unwrap()
                    .insert("tools".into(), tools.clone());
            }
            msgs
        };
        let encoded = encode_messages(&mut messages, mode, true, "low").unwrap();
        let gold = read_fixture(output_name).replace("\r\n", "\n");
        assert_eq!(encoded, gold, "{input_name} encoding mismatch");
    }

    #[test]
    fn official_case_1_thinking_with_tools() {
        official_case("test_input_1.json", "test_output_1.txt", ThinkingMode::Thinking);
    }

    #[test]
    fn official_case_2_thinking_without_tools_drops_earlier_reasoning() {
        official_case("test_input_2.json", "test_output_2.txt", ThinkingMode::Thinking);
        let encoded = {
            let input: Value = serde_json::from_str(&read_fixture("test_input_2.json")).unwrap();
            let mut messages = input.as_array().unwrap().clone();
            encode_messages(&mut messages, ThinkingMode::Thinking, true, "low").unwrap()
        };
        assert!(!encoded.contains("The user said hello"));
    }

    #[test]
    fn official_case_3_interleaved_thinking_search() {
        official_case("test_input_3.json", "test_output_3.txt", ThinkingMode::Thinking);
    }

    #[test]
    fn official_case_4_chat_mode_quick_instruction() {
        official_case("test_input_4.json", "test_output_4.txt", ThinkingMode::Chat);
    }

    #[test]
    fn h_thinking_and_non_thinking_are_distinct() {
        let mut messages = vec![
            json_object([
                ("role", Value::String("system".into())),
                ("content", Value::String("You are a helpful assistant.".into())),
            ]),
            json_object([
                ("role", Value::String("user".into())),
                ("content", Value::String("What is 2+2?".into())),
            ]),
        ];
        let thinking =
            encode_messages(&mut messages.clone(), ThinkingMode::Thinking, true, "low").unwrap();
        let chat = encode_messages(&mut messages, ThinkingMode::Chat, true, "low").unwrap();
        assert_ne!(thinking, chat);
        assert!(thinking.ends_with("<｜Assistant｜><think>"));
        assert!(chat.ends_with("<｜Assistant｜></think>"));
        assert_eq!(
            thinking,
            "<｜begin▁of▁sentence｜>You are a helpful assistant.<｜User｜>What is 2+2?<｜Assistant｜><think>"
        );
    }

    #[test]
    fn f_simple_chat_exact_tokenizer_path_works() {
        let messages = vec![
            ChatMessage::system("You are a helpful assistant."),
            ChatMessage::user("hello"),
        ];
        let counted = count_deepseek_v4_flash_tokens(
            &messages,
            &[],
            DeepSeekV4RequestSettings {
                thinking_mode: ThinkingMode::Chat,
                reasoning_effort: ReasoningEffort::Low,
            },
        )
        .unwrap();
        assert!(counted.tokens > 0);
        assert!(!counted.exact);
        assert_eq!(counted.source, "exact_local_tokenizer");
        let encoded = encode_omninova_messages(
            &messages,
            &[],
            DeepSeekV4RequestSettings {
                thinking_mode: ThinkingMode::Chat,
                reasoning_effort: ReasoningEffort::Low,
            },
        )
        .unwrap();
        assert!(encoded.contains("You are a helpful assistant."));
        assert!(encoded.contains("hello"));
        assert!(!encoded.contains("{\"model\""));
    }

    #[test]
    fn g_chinese_and_english_are_encoded() {
        let messages = vec![ChatMessage::user("你好 world")];
        let encoded = encode_omninova_messages(
            &messages,
            &[],
            DeepSeekV4RequestSettings::default(),
        )
        .unwrap();
        assert!(encoded.contains("你好 world"));
        let counted =
            count_deepseek_v4_flash_tokens(&messages, &[], DeepSeekV4RequestSettings::default())
                .unwrap();
        assert!(counted.tokens > 0);
    }

    #[test]
    fn i_j_k_l_tools_tool_calls_and_visible_results() {
        let tools = vec![ToolSpec {
            name: "get_weather".into(),
            description: "Get the weather".into(),
            parameters: serde_json::json!({"type":"object","properties":{"location":{"type":"string"}}}),
        }];
        let mut result = ChatMessage::tool(
            serde_json::json!({"tool_call_id":"call-1","content":"VISIBLE_RESULT"}).to_string(),
        );
        result.original_tool_content = Some("HIDDEN_ORIGINAL".into());
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("weather?"),
            ChatMessage::assistant(
                serde_json::json!({
                    "content":"",
                    "reasoning_content":"need tool",
                    "tool_calls":[{"id":"call-1","name":"get_weather","arguments":"{\"location\":\"Beijing\"}"}]
                })
                .to_string(),
            ),
            result,
        ];
        let encoded = encode_omninova_messages(
            &messages,
            &tools,
            DeepSeekV4RequestSettings {
                thinking_mode: ThinkingMode::Thinking,
                reasoning_effort: ReasoningEffort::Low,
            },
        )
        .unwrap();
        assert!(encoded.contains("## Tools"));
        assert!(encoded.contains("get_weather"));
        assert!(encoded.contains("<｜DSML｜tool_calls>"));
        assert!(encoded.contains("VISIBLE_RESULT"));
        assert!(!encoded.contains("HIDDEN_ORIGINAL"));
        assert!(encoded.contains("<tool_result>VISIBLE_RESULT</tool_result>"));
    }

    #[test]
    fn m_unsupported_images_fail_closed() {
        let messages = vec![ChatMessage::user_with_images(
            "look",
            vec!["data:image/png;base64,AAAA".into()],
        )];
        assert!(matches!(
            encode_omninova_messages(&messages, &[], DeepSeekV4RequestSettings::default()),
            Err(DeepSeekV4Unsupported::Images)
        ));
    }

    #[test]
    fn tokenizer_is_loaded_once_and_deterministic() {
        assert_eq!(CANONICAL_MODEL, "deepseek-v4-flash");
        assert_eq!(TOKENIZER_FAMILY, "deepseek_v4");
        assert_eq!(TOKENIZER_REVISION, "DeepSeek-V4-Flash-0731");
        assert_eq!(TOKENIZER_NAME, "deepseek_v4_flash_0731");
        let messages = vec![ChatMessage::user("count me")];
        let a = count_deepseek_v4_flash_tokens(
            &messages,
            &[],
            DeepSeekV4RequestSettings::default(),
        )
        .unwrap();
        let b = count_deepseek_v4_flash_tokens(
            &messages,
            &[],
            DeepSeekV4RequestSettings::default(),
        )
        .unwrap();
        assert_eq!(a.tokens, b.tokens);
    }

    #[test]
    fn o_p_safety_estimator_formula_unchanged() {
        let estimator = TokenEstimator::new();
        let text = "hello";
        assert_eq!(
            estimator.estimate_text(text),
            text.chars().count() as u64 + (text.len() / 4) as u64 + 4
        );
        assert_eq!(
            estimator.estimate_request(text),
            estimator.estimate_text(text) + 8
        );
        let messages = vec![ChatMessage::user("hello")];
        assert_eq!(
            estimator.estimate_messages_with_tools(&messages, &[]),
            estimator.estimate_text("hello") + 8
        );
    }
}

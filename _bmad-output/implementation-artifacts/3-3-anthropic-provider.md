# Story 3.3: Anthropic Provider 实现

Status: done

## Story

As a **用户**,
I want **连接 Anthropic Claude API 进行 AI 对话**,
so that **我可以使用 Claude 系列模型为 AI 代理提供推理能力**.

## Acceptance Criteria

1. **AC1: Messages API Implementation** - Implement native Anthropic Messages API client (not OpenAI-compatible)
2. **AC2: Streaming Support** - Implement `chat_stream()` method for real-time response streaming
3. **AC3: Model Configuration** - Support different Claude models (claude-3-opus, claude-3-sonnet, claude-3-haiku, claude-3.5-sonnet)
4. **AC4: Message Format Handling** - Correctly handle Anthropic-specific message format (system parameter, content blocks)
5. **AC5: Tool Calls Support** - Implement tool/function calling with Anthropic's format
6. **AC6: Support Flags** - Override `supports_streaming()` to return `true`, `supports_embeddings()` returns `false`
7. **AC7: Error Handling** - Handle Anthropic-specific error types (overloaded_error, rate_limit_error, etc.)
8. **AC8: Unit Tests** - Comprehensive test coverage for all methods

## Tasks / Subtasks

- [x] Task 1: Native Anthropic API Implementation (AC: #1, #3, #4)
  - [x] 1.1 Create native request/response structs for Anthropic Messages API
  - [x] 1.2 Implement system prompt handling (separate `system` parameter, not in messages)
  - [x] 1.3 Implement content blocks (text, image, tool_use, tool_result)
  - [x] 1.4 Implement `chat()` method calling `/v1/messages` endpoint
  - [x] 1.5 Support model configuration (claude-3-opus, claude-3-sonnet, claude-3-haiku, claude-3-5-sonnet)
  - [x] 1.6 Add unit tests for message formatting and API calls

- [x] Task 2: Streaming Implementation (AC: #2, #6)
  - [x] 2.1 Create streaming event types (message_start, content_block_delta, message_delta, message_stop)
  - [x] 2.2 Implement `chat_stream()` method using SSE
  - [x] 2.3 Parse streaming events and yield `ChatStreamChunk` items
  - [x] 2.4 Override `supports_streaming()` to return `true`
  - [x] 2.5 Add unit tests for streaming (mock SSE responses)

- [x] Task 3: Tool Calls Support (AC: #5)
  - [x] 3.1 Implement tool definition conversion (Anthropic uses different schema)
  - [x] 3.2 Handle `tool_use` content blocks in responses
  - [x] 3.3 Handle `tool_result` content blocks in requests
  - [x] 3.4 Add unit tests for tool calls

- [x] Task 4: Error Handling (AC: #7)
  - [x] 4.1 Add specific error types for Anthropic errors
  - [x] 4.2 Handle rate limit errors with retry-after
  - [x] 4.3 Handle authentication errors
  - [x] 4.4 Handle overloaded errors
  - [x] 4.5 Add Chinese error messages
  - [x] 4.6 Add unit tests for error scenarios

- [x] Task 5: Model Listing & Support Flags (AC: #6)
  - [x] 5.1 Implement `list_models()` returning Claude model list
  - [x] 5.2 Set `supports_embeddings()` to return `false` (Anthropic has no embeddings API)
  - [x] 5.3 Add model capability detection (context length, vision, tools)
  - [x] 5.4 Add unit tests

## Dev Notes

### Existing Implementation

**IMPORTANT:** There's a minimal `AnthropicProvider` in `src/providers/anthropic.rs` that wraps `OpenAiProvider`:

```rust
pub struct AnthropicProvider {
    inner: OpenAiProvider,
}
```

This is a placeholder using OpenAI-compatible endpoints. The story requires **replacing** this with a native Anthropic Messages API implementation.

### Anthropic Messages API Differences from OpenAI

1. **Endpoint**: `POST https://api.anthropic.com/v1/messages`

2. **Headers**:
   ```
   x-api-key: <api_key>
   anthropic-version: 2023-06-01
   Content-Type: application/json
   ```

3. **System Prompt**: Separate `system` parameter at top level, NOT in messages array:
   ```json
   {
     "model": "claude-3-5-sonnet-20241022",
     "max_tokens": 1024,
     "system": "You are a helpful assistant.",
     "messages": [
       {"role": "user", "content": "Hello"}
     ]
   }
   ```

4. **Content Blocks**: Messages use content blocks instead of simple strings:
   ```json
   {
     "role": "user",
     "content": [
       {"type": "text", "text": "Hello"}
     ]
   }
   ```

5. **Tool Calls**:
   - Request: `tools` array with `input_schema` (not `parameters`)
   - Response: `tool_use` content blocks
   - Follow-up: `tool_result` content blocks

6. **Streaming Events**: Different from OpenAI SSE:
   ```
   event: message_start
   data: {"type":"message_start","message":{"id":"msg_xxx",...}}

   event: content_block_start
   data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

   event: content_block_delta
   data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

   event: content_block_stop
   data: {"type":"content_block_stop","index":0}

   event: message_delta
   data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}

   event: message_stop
   data: {"type":"message_stop"}
   ```

### Anthropic Request/Response Structures

```rust
// Request
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

struct AnthropicMessage {
    role: String,  // "user" or "assistant"
    content: Vec<AnthropicContentBlock>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    text { text: String },
    image { source: ImageSource },
    tool_use { id: String, name: String, input: serde_json::Value },
    tool_result { tool_use_id: String, content: String },
}

// Response
struct AnthropicResponse {
    id: String,
    type: String,  // "message"
    role: String,  // "assistant"
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}
```

### Claude Model Capabilities

| Model ID | Tools | Vision | Context |
|----------|-------|--------|---------|
| claude-3-opus-20240229 | Yes | Yes | 200K |
| claude-3-sonnet-20240229 | Yes | Yes | 200K |
| claude-3-haiku-20240307 | Yes | Yes | 200K |
| claude-3-5-sonnet-20241022 | Yes | Yes | 200K |
| claude-3-5-haiku-20241022 | Yes | No | 200K |

### Error Types from Anthropic

```json
{
  "type": "error",
  "error": {
    "type": "overloaded_error",
    "message": "Overloaded"
  }
}
```

Error types:
- `invalid_request_error` - Bad request format
- `authentication_error` - Invalid API key
- `permission_error` - Not authorized
- `not_found_error` - Resource not found
- `rate_limit_error` - Too many requests
- `api_error` - Internal server error
- `overloaded_error` - Service overloaded

### Architecture Patterns to Follow

From Story 3.2 (OpenAI Provider) completion:

1. **Error Messages** - Use Chinese for user-facing error messages:
   ```rust
   anyhow::anyhow!("请求超时：调用 Anthropic API 超时（60s），请检查网络连通性")
   ```

2. **Testing Standards**:
   - Target 30+ tests for this module
   - Use `#[tokio::test]` for async tests
   - Mock external dependencies
   - Test error paths and edge cases

3. **Code Structure** - Follow `openai.rs` pattern:
   - Native types at top
   - Provider struct with fields: `base_url`, `credential`, `model`, `temperature`, `max_tokens`, `client`
   - Private helper methods
   - Provider trait implementation
   - Unit tests in `#[cfg(test)] mod tests`

### Dependencies

Already available from Story 3.2:
- `reqwest` - HTTP client with streaming support
- `tokio` - Async runtime
- `futures-util` - Stream utilities
- `tokio-stream` - For `ReceiverStream` wrapper
- `serde` / `serde_json` - JSON serialization

### Special Considerations

1. **No Embeddings API**: Anthropic doesn't have an embeddings endpoint. `supports_embeddings()` should return `false`.

2. **Max Tokens Required**: Anthropic requires `max_tokens` parameter. Default to 4096 if not specified.

3. **System Message**: Must be extracted from the messages array and passed as separate `system` parameter.

4. **Tool Schema Conversion**: Convert OpenAI-style tool definitions to Anthropic format:
   - `parameters` → `input_schema`
   - Different JSON Schema requirements

### Project Structure Notes

### Files to Modify

- `crates/omninova-core/src/providers/anthropic.rs` - Replace wrapper with native implementation

### Files to Reference

- `crates/omninova-core/src/providers/traits.rs` - Provider trait definition
- `crates/omninova-core/src/providers/openai.rs` - Reference implementation for patterns

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L656-L671] - Story 3.3 requirements
- [Source: crates/omninova-core/src/providers/traits.rs] - Provider trait definition
- [Source: crates/omninova-core/src/providers/openai.rs] - Reference implementation
- [Source: _bmad-output/implementation-artifacts/3-2-openai-provider.md] - Previous story learnings
- [Anthropic API Docs: Messages](https://docs.anthropic.com/en/api/messages)
- [Anthropic API Docs: Streaming](https://docs.anthropic.com/en/api/streaming)
- [Anthropic API Docs: Tool Use](https://docs.anthropic.com/en/docs/tool-use)

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

N/A

### Completion Notes List

1. **Native Anthropic API Implementation**: Replaced the placeholder wrapper with a complete native implementation using the Anthropic Messages API format. Created structs for `AnthropicRequest`, `AnthropicMessage`, `AnthropicContentBlock`, `AnthropicResponse`, and streaming event types.

2. **System Prompt Handling**: Implemented proper extraction of system messages from the messages array and passed them as the separate `system` parameter required by Anthropic's API.

3. **Content Blocks**: Implemented all content block types: `text`, `image`, `tool_use`, and `tool_result`. The `tool_use` blocks are parsed into `ToolCall` structs, and `tool_result` blocks can be sent back for tool call results.

4. **Streaming Implementation**: Implemented `chat_stream()` with SSE parsing for all Anthropic streaming events: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`, `error`, and `ping`.

5. **Tool Calls Support**: Implemented tool definition conversion from OpenAI-style `parameters` to Anthropic's `input_schema`, and proper handling of `tool_use` and `tool_result` content blocks.

6. **Error Handling**: Implemented comprehensive error handling with Chinese messages for:
   - `authentication_error` - Invalid API key
   - `permission_error` - Not authorized
   - `rate_limit_error` - Too many requests
   - `overloaded_error` - Service overloaded
   - `invalid_request_error` - Bad request format

7. **Model Listing**: Implemented `list_models()` returning static list of 5 Claude models with capability detection for tools, vision, and context length.

8. **Support Flags**: `supports_streaming()` returns `true`, `supports_embeddings()` returns `false` (Anthropic doesn't have an embeddings API).

9. **Test Coverage**: 37 unit tests covering all functionality including request/response serialization, streaming events, error handling, message conversion, and model capabilities.

### File List

- `crates/omninova-core/src/providers/anthropic.rs` - Complete native implementation (~960 lines)
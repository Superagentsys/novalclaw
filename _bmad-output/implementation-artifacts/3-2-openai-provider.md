# Story 3.2: OpenAI Provider 实现

Status: done

## Story

As a **用户**,
I want **连接 OpenAI API 进行 AI 对话**,
so that **我可以使用 GPT 系列模型为 AI 代理提供推理能力**.

## Acceptance Criteria

1. **AC1: Streaming Support** - Implement `chat_stream()` method with SSE (Server-Sent Events) for real-time response streaming
2. **AC2: Embeddings Support** - Implement `embeddings()` method for text embeddings using OpenAI's embedding API
3. **AC3: Model Listing** - Implement `list_models()` method to retrieve available models from OpenAI API
4. **AC4: Support Flags** - Override `supports_streaming()` and `supports_embeddings()` to return `true`
5. **AC5: Custom Base URL** - Continue support for custom `base_url` for proxies or Azure OpenAI
6. **AC6: Error Handling** - Comprehensive error handling for rate limits, network errors, authentication failures
7. **AC7: Unit Tests** - Comprehensive test coverage for new methods

## Tasks / Subtasks

- [x] Task 1: Streaming Implementation (AC: #1, #4)
  - [x] 1.1 Add `stream` field to `NativeChatRequest` for streaming requests
  - [x] 1.2 Create `NativeStreamResponse` struct for SSE parsing
  - [x] 1.3 Implement `chat_stream()` method using `reqwest::Response::bytes_stream()`
  - [x] 1.4 Parse SSE events and yield `ChatStreamChunk` items
  - [x] 1.5 Handle streaming errors (connection, parse, rate limit)
  - [x] 1.6 Override `supports_streaming()` to return `true`
  - [x] 1.7 Add unit tests for streaming (mock SSE responses)

- [x] Task 2: Embeddings Implementation (AC: #2, #4)
  - [x] 2.1 Create `NativeEmbeddingRequest` and `NativeEmbeddingResponse` structs
  - [x] 2.2 Implement `embeddings()` method calling `/embeddings` endpoint
  - [x] 2.3 Support model override (text-embedding-3-small, text-embedding-3-large, text-embedding-ada-002)
  - [x] 2.4 Override `supports_embeddings()` to return `true`
  - [x] 2.5 Add unit tests for embeddings

- [x] Task 3: Model Listing Implementation (AC: #3)
  - [x] 3.1 Create `NativeModelsResponse` struct for `/models` response
  - [x] 3.2 Implement `list_models()` method
  - [x] 3.3 Parse response into `ModelInfo` structs with capability detection
  - [x] 3.4 Add unit tests for model listing

- [x] Task 4: Error Handling Enhancement (AC: #6)
  - [x] 4.1 Add specific error types for rate limiting (429)
  - [x] 4.2 Add retry-after header parsing for rate limits
  - [x] 4.3 Improve authentication error messages (401)
  - [x] 4.4 Add timeout handling with clear messages
  - [x] 4.5 Add unit tests for error scenarios

- [x] Task 5: Integration Tests (AC: #7)
  - [x] 5.1 Add integration test with mock server for streaming
  - [x] 5.2 Add integration test with mock server for embeddings
  - [x] 5.3 Add integration test with mock server for model listing
  - [x] 5.4 Test error scenarios with mock responses

## Dev Notes

### Existing Implementation

**IMPORTANT:** `OpenAiProvider` is already implemented with significant functionality:

1. **Struct Definition** (`src/providers/openai.rs:10-17`):
   ```rust
   pub struct OpenAiProvider {
       base_url: String,
       credential: Option<String>,
       model: String,
       temperature: f64,
       max_tokens: Option<u32>,
       client: Client,
   }
   ```

2. **Implemented Methods**:
   - `new()` - Constructor with custom base_url, credential, model, temperature, max_tokens
   - `chat()` - Full implementation with tool calls support
   - `health_check()` - Calls `/models` endpoint to verify connectivity
   - Private helpers: `convert_tools()`, `convert_messages()`, `parse_native_response()`

3. **Native Types** (already defined):
   - `NativeChatRequest`, `NativeMessage`, `NativeToolSpec`, `NativeToolCall`
   - `NativeChatResponse`, `NativeChoice`, `NativeResponseMessage`
   - `UsageInfo` for token usage

4. **Error Handling** (`src/providers/openai.rs:114-118`):
   ```rust
   async fn api_error(provider_name: &str, response: Response) -> anyhow::Error
   ```

### Provider Trait Requirements

From `src/providers/traits.rs`, the `Provider` trait has these methods with default implementations:

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse>;
    async fn health_check(&self) -> bool;

    // Default implementations return errors - need to override:
    async fn chat_stream(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
        anyhow::bail!("Streaming not supported by provider '{}'", self.name())
    }
    async fn embeddings(&self, _request: EmbeddingRequest<'_>) -> anyhow::Result<EmbeddingResponse> {
        anyhow::bail!("Embeddings not supported by provider '{}'", self.name())
    }
    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(Vec::new())
    }

    // Override to true:
    fn supports_streaming(&self) -> bool { false }
    fn supports_embeddings(&self) -> bool { false }
}
```

### Streaming Implementation Guide

OpenAI uses SSE (Server-Sent Events) format for streaming:

1. **Request Format** - Add `stream: true` to request:
   ```json
   {
     "model": "gpt-4o-mini",
     "messages": [...],
     "stream": true
   }
   ```

2. **Response Format** - SSE events:
   ```
   data: {"id":"chatcmpl-xxx","choices":[{"delta":{"content":"Hello"},"index":0}]}
   data: {"id":"chatcmpl-xxx","choices":[{"delta":{"content":" world"},"index":0}]}
   data: {"id":"chatcmpl-xxx","choices":[{"delta":{},"finish_reason":"stop","index":0}]}
   data: [DONE]
   ```

3. **Implementation Pattern**:
   ```rust
   use futures_util::StreamExt;
   use tokio_stream::wrappers::ReceiverStream;

   async fn chat_stream(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
       // Build request with stream: true
       let response = self.client
           .post(format!("{}/chat/completions", self.base_url))
           .header("Authorization", format!("Bearer {}", credential))
           .json(&native_request)
           .send()
           .await?;

       // Create channel for chunks
       let (tx, rx) = tokio::sync::mpsc::channel(32);

       // Spawn task to process SSE stream
       tokio::spawn(async move {
           let mut stream = response.bytes_stream();
           let mut buffer = String::new();

           while let Some(chunk) = stream.next().await {
               match chunk {
                   Ok(bytes) => {
                       buffer.push_str(&String::from_utf8_lossy(&bytes));
                       // Parse SSE events from buffer
                       // Send ChatStreamChunk to channel
                   }
                   Err(e) => {
                       let _ = tx.send(Err(StreamError::Connection(e.to_string()))).await;
                       break;
                   }
               }
           }
       });

       Ok(Box::pin(ReceiverStream::new(rx)))
   }
   ```

### Embeddings API

OpenAI Embeddings API endpoint: `POST /embeddings`

```json
// Request
{
  "model": "text-embedding-3-small",
  "input": "The text to embed"
}

// Response
{
  "object": "list",
  "data": [{
    "object": "embedding",
    "index": 0,
    "embedding": [0.1, 0.2, ...]
  }],
  "model": "text-embedding-3-small",
  "usage": {
    "prompt_tokens": 5,
    "total_tokens": 5
  }
}
```

### Models API

OpenAI Models API endpoint: `GET /models`

```json
// Response
{
  "object": "list",
  "data": [
    {
      "id": "gpt-4o-mini",
      "object": "model",
      "created": 1721172741,
      "owned_by": "system"
    },
    ...
  ]
}
```

### Model Capability Detection

Detect model capabilities from model ID patterns:

| Pattern | Tools | Vision | Context |
|---------|-------|--------|---------|
| `gpt-4o*` | Yes | Yes | 128K |
| `gpt-4-turbo*` | Yes | Yes | 128K |
| `gpt-4-*` | Yes | No | 8K/32K |
| `gpt-3.5-turbo*` | Yes | No | 16K |
| `o1-*`, `o3-*` | No | Varies | 200K |
| `chatgpt-4o-*` | Yes | Yes | 128K |

### Architecture Patterns to Follow

From Epic 2 retrospective:

1. **Error Messages** - Use Chinese for user-facing error messages:
   ```rust
   anyhow::anyhow!("请求超时：调用 {} 超时（60s），请检查网络连通性", self.base_url)
   ```

2. **Testing Standards**:
   - Target 100+ tests for Rust modules
   - Use `#[tokio::test]` for async tests
   - Mock external dependencies
   - Test error paths and edge cases

3. **Existing Pattern in `chat()`**:
   ```rust
   async fn chat(&self, request: ProviderChatRequest<'_>) -> anyhow::Result<ProviderChatResponse> {
       let credential = self.credential.as_ref().ok_or_else(|| {
           anyhow::anyhow!("OpenAI API key not set. Set OPENAI_API_KEY or configure api_key.")
       })?;
       // ... request building
       let response = self.client
           .post(format!("{}/chat/completions", self.base_url))
           .header("Authorization", format!("Bearer {credential}"))
           .json(&native_request)
           .send()
           .await
           .map_err(|e| {
               if e.is_timeout() {
                   anyhow::anyhow!("请求超时：调用 {} 超时（60s）...", self.base_url)
               } else if e.is_connect() {
                   anyhow::anyhow!("连接失败：无法连接到 {}...", self.base_url)
               } else {
                   anyhow::anyhow!("网络请求失败：{}", e)
               }
           })?;
       // ... response handling
   }
   ```

### Dependencies

Already available in `Cargo.toml`:
- `reqwest` - HTTP client with streaming support
- `tokio` - Async runtime
- `futures-util` - Stream utilities
- `serde` / `serde_json` - JSON serialization
- `async-trait` - Trait async support
- `anyhow` - Error handling
- `uuid` - UUID generation

May need to add:
- `tokio-stream` - For `ReceiverStream` wrapper (check if already in dependencies)

### Special Considerations

1. **Temperature Handling** - Some models require default temperature:
   ```rust
   fn model_requires_default_temperature(model: &str) -> bool {
       let normalized = model.trim().to_ascii_lowercase();
       normalized.starts_with("gpt-5")
           || normalized.starts_with("o1")
           || normalized.starts_with("o3")
           || normalized.starts_with("o4")
   }
   ```

2. **Reasoning Content** - Support `reasoning_content` field for thinking models (DeepSeek, Kimi, GLM).

3. **Tool Calls in Streaming** - Tool calls may come in the final chunk of streaming responses.

## Project Structure Notes

### Files to Modify
- `crates/omninova-core/src/providers/openai.rs` - Add streaming, embeddings, model listing

### Files to Reference
- `crates/omninova-core/src/providers/traits.rs` - Provider trait definition
- `crates/omninova-core/src/providers/config.rs` - ProviderType enum
- `crates/omninova-core/src/providers/registry.rs` - Provider registration

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L639-L655] - Story 3.2 requirements
- [Source: crates/omninova-core/src/providers/openai.rs] - Existing OpenAI implementation
- [Source: crates/omninova-core/src/providers/traits.rs] - Provider trait with streaming/embeddings
- [Source: _bmad-output/implementation-artifacts/3-1-llm-provider-trait.md] - Story 3.1 completions
- [Source: _bmad-output/implementation-artifacts/epic-2-retrospective.md] - Established patterns
- [OpenAI API Docs: Chat Completions](https://platform.openai.com/docs/api-reference/chat)
- [OpenAI API Docs: Embeddings](https://platform.openai.com/docs/api-reference/embeddings)
- [OpenAI API Docs: Models](https://platform.openai.com/docs/api-reference/models)

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

N/A

### Completion Notes List

1. **Streaming Implementation**: Implemented `chat_stream()` with SSE parsing using `bytes_stream()` from reqwest. Supports text deltas, reasoning content, tool calls, and usage statistics in streaming chunks.

2. **Embeddings Implementation**: Implemented `embeddings()` method with support for model override. Default model is `text-embedding-3-small`.

3. **Model Listing**: Implemented `list_models()` with capability detection for tools, vision, and context length based on model ID patterns.

4. **Error Handling**: Enhanced error handling with:
   - Rate limit (429) with `Retry-After` header parsing
   - Authentication errors (401) with clear Chinese messages
   - Timeout and connection error handling

5. **Bug Fix**: Fixed `detect_model_capabilities()` to correctly detect `chatgpt-4o-*` models for 128K context length.

6. **Test Coverage**: 36 unit tests covering all new functionality. All 320 tests pass.

### File List

- `crates/omninova-core/src/providers/openai.rs` - Main implementation file (~1260 lines)
- `crates/omninova-core/Cargo.toml` - Added `tokio-stream = "0.1"` dependency
- `Cargo.toml` - Added `stream` feature to reqwest
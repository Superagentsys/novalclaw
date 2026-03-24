# Story 3.4: Ollama 本地模型 Provider 实现

Status: done

## Story

As a **用户**,
I want **连接本地 Ollama 服务运行开源模型**,
so that **我可以在本地环境中使用 AI 代理而无需云服务**.

## Acceptance Criteria

1. **AC1: Ollama API Client** - Implement native Ollama API client (default: localhost:11434)
2. **AC2: Model Listing** - Support fetching available local models via `/api/tags` endpoint
3. **AC3: Streaming Support** - Implement `chat_stream()` for real-time response streaming
4. **AC4: Custom Base URL** - Support configurable Ollama service address
5. **AC5: Error Handling** - Handle local service unavailable with meaningful Chinese error messages
6. **AC6: Embeddings Support** - Implement `embeddings()` using Ollama's `/api/embeddings` endpoint
7. **AC7: Support Flags** - Override `supports_streaming()` and `supports_embeddings()` to return `true`
8. **AC8: Unit Tests** - Comprehensive test coverage for all methods

## Tasks / Subtasks

- [x] Task 1: Native Ollama API Implementation (AC: #1, #4)
  - [x] 1.1 Create native request/response structs for Ollama API
  - [x] 1.2 Implement `chat()` method calling `/api/chat` endpoint
  - [x] 1.3 Support custom base URL configuration
  - [x] 1.4 Handle connection errors for local service
  - [x] 1.5 Add unit tests for API calls

- [x] Task 2: Model Listing Implementation (AC: #2)
  - [x] 2.1 Create structs for `/api/tags` response
  - [x] 2.2 Implement `list_models()` method
  - [x] 2.3 Parse model info (name, size, modified time)
  - [x] 2.4 Add unit tests for model listing

- [x] Task 3: Streaming Implementation (AC: #3, #7)
  - [x] 3.1 Create streaming response types for Ollama's NDJSON format
  - [x] 3.2 Implement `chat_stream()` method
  - [x] 3.3 Parse NDJSON streaming events
  - [x] 3.4 Override `supports_streaming()` to return `true`
  - [x] 3.5 Add unit tests for streaming

- [x] Task 4: Embeddings Implementation (AC: #6, #7)
  - [x] 4.1 Create structs for `/api/embeddings` endpoint
  - [x] 4.2 Implement `embeddings()` method
  - [x] 4.3 Override `supports_embeddings()` to return `true`
  - [x] 4.4 Add unit tests for embeddings

- [x] Task 5: Error Handling (AC: #5)
  - [x] 5.1 Handle connection refused errors
  - [x] 5.2 Handle timeout errors
  - [x] 5.3 Add Chinese error messages
  - [x] 5.4 Add unit tests for error scenarios

## Dev Notes

### Existing Implementation

**IMPORTANT:** There's currently no dedicated `OllamaProvider` - the registry uses `OpenAiProvider` with OpenAI-compatible endpoint:

```rust
// Ollama provider (OpenAI-compatible with default URL)
factories.insert(
    "ollama".to_string(),
    Box::new(|base_url, api_key, model, temp| {
        let url = base_url.or(Some("http://localhost:11434/v1"));
        Box::new(OpenAiProvider::new(
            url,
            api_key,
            model.unwrap_or("llama3.2").to_string(),
            temp as f64,
            None,
        ))
    }),
);
```

This works for basic chat via OpenAI-compatible `/v1/chat/completions`, but lacks:
- Native Ollama API features (`/api/chat`, `/api/tags`, `/api/embeddings`)
- Model listing via `/api/tags`
- Native embeddings via `/api/embeddings`
- Ollama-specific error handling

### Ollama API Endpoints

1. **Chat API**: `POST /api/chat`
   ```json
   {
     "model": "llama3.2",
     "messages": [{"role": "user", "content": "Hello"}],
     "stream": false
   }
   ```

2. **Streaming Chat**: Set `"stream": true`, returns NDJSON:
   ```json
   {"model":"llama3.2","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Hello"},"done":false}
   {"model":"llama3.2","created_at":"2024-01-01T00:00:01Z","message":{"role":"assistant","content":" there"},"done":false}
   {"model":"llama3.2","created_at":"2024-01-01T00:00:02Z","message":{"role":"assistant","content":"!"},"done":true,"done_reason":"stop"}
   ```

3. **List Models**: `GET /api/tags`
   ```json
   {
     "models": [
       {
         "name": "llama3.2:latest",
         "modified_at": "2024-01-01T00:00:00Z",
         "size": 4661224676,
         "digest": "abc123",
         "details": {"format": "gguf", "family": "llama"}
       }
     ]
   }
   ```

4. **Embeddings**: `POST /api/embeddings`
   ```json
   {
     "model": "nomic-embed-text",
     "prompt": "Hello world"
   }
   ```
   Response:
   ```json
   {
     "embedding": [0.1, 0.2, ...]
   }
   ```

### Ollama Request/Response Structures

```rust
// Request
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

struct OllamaMessage {
    role: String,  // "system", "user", "assistant"
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,  // base64 encoded
}

// Response
struct OllamaChatResponse {
    model: String,
    created_at: String,
    message: OllamaMessage,
    done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_eval_count: Option<u64>,
}

// Models List
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

struct OllamaModel {
    name: String,
    modified_at: String,
    size: u64,
    digest: String,
    details: Option<OllamaModelDetails>,
}

// Embeddings
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}
```

### Architecture Patterns to Follow

From Story 3.2 & 3.3:

1. **Error Messages** - Use Chinese for user-facing error messages:
   ```rust
   anyhow::anyhow!("连接失败：无法连接到 Ollama 服务（{}），请确认 Ollama 是否正在运行", self.base_url)
   ```

2. **Testing Standards**:
   - Target 30+ tests for this module
   - Use `#[tokio::test]` for async tests
   - Mock external dependencies
   - Test error paths and edge cases

3. **Code Structure** - Follow `openai.rs` / `anthropic.rs` pattern:
   - Native types at top
   - Provider struct with fields: `base_url`, `model`, `temperature`, `client`
   - Private helper methods
   - Provider trait implementation
   - Unit tests in `#[cfg(test)] mod tests`

### Dependencies

Already available:
- `reqwest` - HTTP client with streaming support
- `tokio` - Async runtime
- `futures-util` - Stream utilities
- `tokio-stream` - For `ReceiverStream` wrapper
- `serde` / `serde_json` - JSON serialization

### Special Considerations

1. **No API Key Required**: Ollama runs locally without authentication.

2. **NDJSON Streaming**: Ollama uses newline-delimited JSON, not SSE like OpenAI/Anthropic.

3. **Model Names**: Include tags (e.g., "llama3.2:latest", "llama3.2:3b").

4. **Connection Handling**: Must handle connection refused gracefully since it's a local service.

5. **Default URL**: `http://localhost:11434` (note: no `/v1` for native API).

### Files to Create

- `crates/omninova-core/src/providers/ollama.rs` - New file with native implementation

### Files to Modify

- `crates/omninova-core/src/providers/mod.rs` - Add `pub mod ollama;` and export
- `crates/omninova-core/src/providers/registry.rs` - Update to use `OllamaProvider`

### Files to Reference

- `crates/omninova-core/src/providers/traits.rs` - Provider trait definition
- `crates/omninova-core/src/providers/openai.rs` - Reference implementation for patterns
- `crates/omninova-core/src/providers/anthropic.rs` - Reference implementation for patterns

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L673-L688] - Story 3.4 requirements
- [Source: crates/omninova-core/src/providers/traits.rs] - Provider trait definition
- [Source: crates/omninova-core/src/providers/registry.rs] - Current Ollama registration
- [Ollama API Docs](https://github.com/ollama/ollama/blob/main/docs/api.md)

## Dev Agent Record

### Agent Model Used

<!-- Will be filled during implementation -->

### Debug Log References

N/A

### Completion Notes List

<!-- Will be filled during implementation -->

### File List

<!-- Will be filled during implementation -->
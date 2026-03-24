# Story 3.1: LLM Provider Trait 定义

Status: completed

## Story

As a **developer**,
I want **a unified LLM Provider trait with streaming, embeddings, and model listing capabilities**,
so that **the application can seamlessly switch between different LLM providers with consistent behavior**.

## Acceptance Criteria

1. **AC1: Provider Trait Enhancement** - Extend existing `Provider` trait with `chat_stream`, `embeddings`, and `list_models` methods
2. **AC2: ProviderConfig Database Storage** - Create `provider_configs` table with SQLite migration for storing provider settings
3. **AC3: ProviderStore Implementation** - Implement Store pattern for provider configuration CRUD operations
4. **AC4: ProviderRegistry Pattern** - Create registry for dynamic provider registration and instantiation
5. **AC5: Streaming Support** - Add `ChatStreamChunk` structure for streaming response handling
6. **AC6: Embeddings Support** - Add `EmbeddingRequest` and `EmbeddingResponse` structures
7. **AC7: Unit Tests** - Comprehensive test coverage for new trait methods and data structures

## Tasks / Subtasks

- [x] Task 1: Extend Provider Trait (AC: #1, #5, #6)
  - [x] 1.1 Add `chat_stream()` method with async generator pattern
  - [x] 1.2 Add `embeddings()` method for text embeddings
  - [x] 1.3 Add `list_models()` method to retrieve available models
  - [x] 1.4 Define `ChatStreamChunk` struct for streaming chunks
  - [x] 1.5 Define `EmbeddingRequest` and `EmbeddingResponse` structs
  - [x] 1.6 Add unit tests for new data structures

- [x] Task 2: Database Schema for ProviderConfig (AC: #2)
  - [x] 2.1 Create migration `005_provider_configs.sql`
  - [x] 2.2 Define `provider_configs` table with id, name, provider_type, api_key_ref, base_url, default_model, settings, created_at, updated_at
  - [x] 2.3 Add seed data for default providers
  - [x] 2.4 Write Rust model `ProviderConfig` with SQL traits
  - [x] 2.5 Add unit tests for ProviderConfig model

- [x] Task 3: ProviderStore Implementation (AC: #3)
  - [x] 3.1 Create `ProviderStore` struct with DbPool
  - [x] 3.2 Implement `create()`, `get()`, `list()`, `update()`, `delete()` methods
  - [x] 3.3 Add `get_default_provider()` method
  - [x] 3.4 Implement secure API key storage using keychain module (from Story 2.13)
  - [x] 3.5 Add unit tests for ProviderStore

- [x] Task 4: ProviderRegistry Pattern (AC: #4)
  - [x] 4.1 Create `ProviderRegistry` struct with HashMap for providers
  - [x] 4.2 Implement `register()` method for dynamic provider registration
  - [x] 4.3 Implement `create_provider()` factory method
  - [x] 4.4 Implement `list_provider_types()` method
  - [x] 4.5 Add unit tests for ProviderRegistry

- [x] Task 5: Tauri Commands (AC: #3, #4)
  - [x] 5.1 Create `get_provider_configs` command
  - [x] 5.2 Create `save_provider_config` command
  - [x] 5.3 Create `delete_provider_config` command
  - [x] 5.4 Create `test_provider_connection` command
  - [x] 5.5 Register commands in lib.rs
  - [x] 5.6 Add integration tests

## Dev Notes

### Existing Implementation

**IMPORTANT:** The providers module already has significant implementation:

1. **Provider Trait** (`src/providers/traits.rs`):
   ```rust
   #[async_trait]
   pub trait Provider: Send + Sync {
       fn name(&self) -> &str;
       async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse>;
       async fn health_check(&self) -> bool;
   }
   ```

2. **Factory Pattern** (`src/providers/factory.rs`):
   - `build_provider_from_config()` - Creates provider from Config
   - `build_provider_with_selection()` - Creates with ProviderSelection override
   - Supports 25+ providers: OpenAI, Anthropic, Gemini, Ollama, DeepSeek, Qwen, Moonshot, Groq, XAI, Mistral, LMStudio, OpenRouter, Together, Fireworks, Novita, Perplexity, Cohere, Doubao, Qianfan, GLM, Minimax, NVIDIA, Cloudflare, SGLang, vLLM, LlamaCpp

3. **Existing Data Structures** (`src/providers/traits.rs`):
   - `ChatMessage` - Message with role and content
   - `ChatRequest` - Request with messages and options
   - `ChatResponse` - Response with content, tool_calls, usage
   - `ToolCall` - Tool invocation structure
   - `TokenUsage` - Token count tracking
   - `ConversationMessage` - Enum for multi-turn conversations

4. **Provider Implementations**:
   - `OpenAiProvider` - Complete with tool calls support
   - `AnthropicProvider` - Anthropic API integration
   - `GeminiProvider` - Google Gemini integration
   - `MockProvider` - Testing mock

### What Needs to Be Added

| Feature | Current Status | Required Action |
|---------|---------------|-----------------|
| `chat_stream()` | Not implemented | Add async streaming method |
| `embeddings()` | Not implemented | Add embeddings method |
| `list_models()` | Not implemented | Add model listing method |
| `ProviderConfig` DB | Not implemented | Create migration and model |
| `ProviderStore` | Not implemented | Implement Store pattern |
| `ProviderRegistry` | Factory exists, no registry | Create registry for dynamic registration |

### Architecture Patterns to Follow

From Epic 2 retrospective, established patterns:

1. **Store Pattern**:
   ```rust
   pub struct ProviderStore { pool: DbPool }
   impl ProviderStore {
       pub async fn create(&self, config: &ProviderConfig) -> Result<String>;
       pub async fn get(&self, id: &str) -> Result<Option<ProviderConfig>>;
       pub async fn list(&self) -> Result<Vec<ProviderConfig>>;
       pub async fn update(&self, config: &ProviderConfig) -> Result<()>;
       pub async fn delete(&self, id: &str) -> Result<()>;
   }
   ```

2. **Enum Design Pattern**:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub enum ProviderType {
       #[serde(rename = "openai")]
       OpenAI,
       #[serde(rename = "anthropic")]
       Anthropic,
       // ...
   }
   impl Display, FromStr, FromSql, ToSql
   ```

3. **Tauri Command Pattern**:
   ```rust
   #[tauri::command]
   async fn get_provider_configs(store: State<'_, Arc<ProviderStore>>) -> Result<String, String>
   ```

4. **Security Pattern** (from Story 2.13):
   - Use OS Keychain for API key storage
   - Reference key in database, don't store actual key
   - AES-256-GCM encryption for sensitive data

### Database Schema Design

```sql
CREATE TABLE provider_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    provider_type TEXT NOT NULL,
    api_key_ref TEXT,  -- Reference to keychain entry
    base_url TEXT,
    default_model TEXT,
    settings TEXT,  -- JSON for provider-specific settings
    is_default INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_provider_configs_type ON provider_configs(provider_type);
```

### Streaming Implementation

Use `futures::Stream` trait for streaming:

```rust
use futures::Stream;

pub struct ChatStreamChunk {
    pub delta: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

async fn chat_stream(
    &self,
    request: ChatRequest<'_>,
) -> anyhow::Result<Pin<Box<dyn Stream<Item = Result<ChatStreamChunk, Error>> + Send>>>;
```

### Testing Standards

From Epic 2 retrospective:
- Target 100+ tests for Rust modules (Story 2.1 had 90 tests, Story 2.2 had 116 tests)
- Use `#[tokio::test]` for async tests
- Mock external dependencies with trait-based mocking
- Test error paths and edge cases

## Project Structure Notes

### Files to Modify
- `crates/omninova-core/src/providers/traits.rs` - Add new trait methods
- `crates/omninova-core/src/providers/mod.rs` - Export new types
- `crates/omninova-core/src/providers/factory.rs` - Integrate with ProviderRegistry

### Files to Create
- `crates/omninova-core/src/providers/config.rs` - ProviderType enum and ProviderConfig model
- `crates/omninova-core/src/providers/store.rs` - ProviderStore implementation
- `crates/omninova-core/src/providers/registry.rs` - ProviderRegistry implementation

### Alignment with Unified Structure
- Follow Store pattern from `agent/store.rs` and `agent/store.rs`
- Use existing DbPool from `db/mod.rs`
- Integrate with security module from Story 2.13

## References

- [Source: _bmad-output/planning-artifacts/epics.md#Epic3] - Story 3.1 requirements
- [Source: _bmad-output/planning-artifacts/architecture.md#providers] - Provider architecture design
- [Source: crates/omninova-core/src/providers/traits.rs] - Existing Provider trait
- [Source: crates/omninova-core/src/providers/factory.rs] - Existing factory implementation
- [Source: crates/omninova-core/src/providers/openai.rs] - OpenAI provider implementation
- [Source: _bmad-output/implementation-artifacts/epic-2-retrospective.md] - Established patterns

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (glm-5)

### Debug Log References

N/A

### Completion Notes List

1. **Task 1 - Provider Trait Enhancement**: Added `chat_stream`, `embeddings`, `list_models` methods to Provider trait with default implementations. Added `ChatStreamChunk`, `EmbeddingRequest`, `EmbeddingResponse`, `ModelInfo`, `StreamError`, `ChatStream` types. 26 unit tests.

2. **Task 2 - Database Schema**: Created migration 005_provider_configs with proper schema for storing provider configurations. Added ProviderConfig, NewProviderConfig, ProviderConfigUpdate, ProviderConfigValidationError types in config.rs. 17 unit tests for config module.

3. **Task 3 - ProviderStore Implementation**: Implemented ProviderStore with CRUD operations using DbPool. Added find_all, find_by_id, find_by_name, find_by_type, find_default, create, update, delete, set_default, exists_by_name methods. 16 unit tests.

4. **Task 4 - ProviderRegistry Pattern**: Created ProviderRegistry with RwLock<HashMap> for thread-safe provider factory storage. Registered 25+ built-in providers. Added global_registry() singleton. 14 unit tests.

5. **Task 5 - Tauri Commands**: Added 8 Tauri commands for provider configuration management: init_provider_store, get_provider_configs, get_provider_config_by_id, create_provider_config, update_provider_config, delete_provider_config, set_default_provider_config, test_provider_connection.

**Total Test Coverage**: 286 tests passing in omninova-core

### File List

**Modified Files:**
- `crates/omninova-core/src/providers/traits.rs` - Extended Provider trait with streaming, embeddings, model listing
- `crates/omninova-core/src/providers/mod.rs` - Added exports for new modules
- `crates/omninova-core/src/db/migrations.rs` - Added migration 005_provider_configs
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Added provider commands and AppState.provider_store

**Created Files:**
- `crates/omninova-core/src/providers/config.rs` - ProviderType enum and ProviderConfig model
- `crates/omninova-core/src/providers/store.rs` - ProviderStore implementation
- `crates/omninova-core/src/providers/registry.rs` - ProviderRegistry implementation

## Code Review

**Review Date**: 2026-03-17

### Issues Found

1. **Medium - Documentation**: Story file listed incorrect migration path (`008_provider_configs.sql` vs actual embedded migration in `migrations.rs`). **Fixed**: Updated File List section to accurately reflect implementation.

2. **Medium - Thread Safety**: `global_registry()` used `unsafe { static mut }` pattern which is not thread-safe. **Fixed**: Replaced with `std::sync::OnceLock<ProviderRegistry>` for safe singleton initialization.

### Post-Fix Verification

- All 14 registry tests passing
- OnceLock pattern ensures thread-safe lazy initialization
- No unsafe code blocks remaining in registry module
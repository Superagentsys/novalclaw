# Story 3.6: Provider 配置界面

Status: completed

## Story

As a **用户**,
I want **通过图形界面配置 LLM 提供商**,
so that **我可以轻松添加和管理不同的 AI 模型提供商**.

## Acceptance Criteria

1. **AC1: Provider List Display** - 显示已配置的提供商列表及其连接状态
2. **AC2: Add Provider** - 可以添加新提供商（选择类型、输入 API 密钥、选择默认模型）
3. **AC3: Edit Provider** - 可以编辑现有提供商配置
4. **AC4: Delete Provider** - 可以删除提供商配置
5. **AC5: Test Connection** - 提供"测试连接"按钮验证配置有效性
6. **AC6: Secure API Key Input** - API 密钥输入框使用密码类型显示
7. **AC7: Keychain Integration** - 成功保存后密钥自动存入系统密钥链

## Tasks / Subtasks

- [x] Task 1: ProviderSettings Page Component (AC: #1)
  - [x] 1.1 Create `apps/omninova-tauri/src/components/settings/ProviderSettings.tsx`
  - [x] 1.2 Implement provider list display with connection status badges
  - [x] 1.3 Add "Add Provider" button with dropdown for provider type selection
  - [x] 1.4 Display default provider indicator and allow setting default
  - [x] 1.5 Add empty state UI when no providers configured
  - [x] 1.6 Implement responsive layout for settings page

- [x] Task 2: Provider CRUD Tauri Commands (AC: #2, #3, #4)
  - [x] 2.1 Add `list_providers` Tauri command in lib.rs
  - [x] 2.2 Add `create_provider` Tauri command with keychain integration
  - [x] 2.3 Add `update_provider` Tauri command
  - [x] 2.4 Add `delete_provider` Tauri command with keychain cleanup
  - [x] 2.5 Add `set_default_provider` Tauri command
  - [x] 2.6 Add TypeScript type definitions for all commands

- [x] Task 3: ProviderForm Dialog Component (AC: #2, #3, #6)
  - [x] 3.1 Create `apps/omninova-tauri/src/components/settings/ProviderFormDialog.tsx`
  - [x] 3.2 Implement provider type selector with category grouping (cloud/local)
  - [x] 3.3 Add display name input field
  - [x] 3.4 Add API key input with password type and visibility toggle
  - [x] 3.5 Add base URL input with default value from provider type
  - [x] 3.6 Add default model selector populated from provider presets
  - [x] 3.7 Add form validation with Chinese error messages
  - [x] 3.8 Implement edit mode that loads existing provider data

- [x] Task 4: Test Connection Feature (AC: #5)
  - [x] 4.1 Add `test_provider_connection` Tauri command
  - [x] 4.2 Implement connection test UI with loading state
  - [x] 4.3 Display success/error results with Chinese messages
  - [x] 4.4 Add connection status badge on provider cards
  - [x] 4.5 Cache connection test results with expiration

- [x] Task 5: Keychain Integration for API Keys (AC: #7)
  - [x] 5.1 Integrate `saveApiKey` from Story 3.5 when saving provider
  - [x] 5.2 Integrate `getApiKey` for displaying saved key status
  - [x] 5.3 Integrate `deleteApiKey` when deleting provider
  - [x] 5.4 Show key storage type indicator (os-keyring/encrypted-file)
  - [x] 5.5 Handle keychain errors gracefully with user-friendly messages

- [x] Task 6: Provider Card Component (AC: #1, #4)
  - [x] 6.1 Create `apps/omninova-tauri/src/components/settings/ProviderCard.tsx`
  - [x] 6.2 Display provider icon, name, type, and connection status
  - [x] 6.3 Add edit and delete action buttons
  - [x] 6.4 Add "Set as Default" toggle button
  - [x] 6.5 Show last tested timestamp
  - [x] 6.6 Implement confirmation dialog for delete action

- [x] Task 7: Unit Tests (All ACs)
  - [x] 7.1 Add unit tests for ProviderSettings component
  - [x] 7.2 Add unit tests for ProviderFormDialog component
  - [x] 7.3 Add unit tests for ProviderCard component
  - [x] 7.4 Add unit tests for Tauri command invocations
  - [x] 7.5 Add integration tests for keychain operations

## Dev Notes

### Existing Implementation Context

**IMPORTANT:** This story builds on multiple existing implementations:

1. **ProviderConfigForm.tsx** exists at `apps/omninova-tauri/src/components/Setup/ProviderConfigForm.tsx` - This is a basic form for setup wizard. The new `ProviderSettings.tsx` will be a more full-featured settings page component.

2. **ProviderStore** exists at `crates/omninova-core/src/providers/store.rs` - Complete CRUD operations for provider configurations with SQLite storage.

3. **KeyringService** from Story 3.5 provides secure API key storage via:
   - `saveApiKey(provider, apiKey)` - Returns key reference URL
   - `getApiKey(provider)` - Retrieves stored key
   - `deleteApiKey(provider)` - Removes stored key
   - `apiKeyExists(provider)` - Checks if key exists

4. **ProviderConfig types** in `crates/omninova-core/src/providers/config.rs`:
   - `ProviderType` enum with 25+ provider types
   - `ProviderConfig` struct with id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default
   - `NewProviderConfig` and `ProviderConfigUpdate` for CRUD operations

### TypeScript Type Definitions

Add to `apps/omninova-tauri/src/types/provider.ts`:

```typescript
// Provider types matching Rust ProviderType enum
export type ProviderType =
  | 'openai' | 'anthropic' | 'gemini'
  | 'ollama' | 'lmstudio' | 'llamacpp' | 'vllm' | 'sglang'
  | 'openrouter' | 'together' | 'fireworks' | 'novita'
  | 'deepseek' | 'qwen' | 'moonshot' | 'doubao' | 'qianfan' | 'glm' | 'minimax'
  | 'groq' | 'xai' | 'mistral' | 'perplexity' | 'cohere' | 'nvidia' | 'cloudflare'
  | 'mock' | 'custom';

export interface ProviderConfig {
  id: string;
  name: string;
  providerType: ProviderType;
  apiKeyRef?: string;
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface NewProviderConfig {
  name: string;
  providerType: ProviderType;
  apiKey?: string; // Will be stored in keychain
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault: boolean;
}

export interface ProviderConfigUpdate {
  name?: string;
  apiKey?: string; // Will update keychain if provided
  baseUrl?: string;
  defaultModel?: string;
  settings?: string;
  isDefault?: boolean;
}

export interface ProviderTestResult {
  success: boolean;
  message: string;
  latencyMs?: number;
}

export interface ProviderWithStatus extends ProviderConfig {
  connectionStatus: 'untested' | 'testing' | 'connected' | 'failed';
  lastTested?: number;
  keyExists: boolean;
  storeType?: 'os-keyring' | 'encrypted-file';
}
```

### Tauri Commands Pattern

Follow existing patterns from `lib.rs`:

```rust
#[tauri::command]
async fn list_providers(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ProviderConfig>, String> {
    let state = state.lock().await;
    let store = state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized".to_string())?;
    store.find_all()
        .map_err(|e| format!("获取提供商列表失败: {}", e))
}

#[tauri::command]
async fn create_provider(
    config: NewProviderConfig,
    api_key: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ProviderConfig, String> {
    // 1. Validate input
    // 2. Store API key in keychain if provided
    // 3. Create provider config with key reference
    // 4. Return created config
}

#[tauri::command]
async fn update_provider(
    id: String,
    update: ProviderConfigUpdate,
    api_key: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ProviderConfig, String> {
    // Implementation
}

#[tauri::command]
async fn delete_provider(
    id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    // 1. Get provider config
    // 2. Delete API key from keychain if exists
    // 3. Delete provider from store
}

#[tauri::command]
async fn test_provider_connection(
    id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ProviderTestResult, String> {
    // 1. Get provider config
    // 2. Get API key from keychain
    // 3. Build provider instance
    // 4. Call health_check()
    // 5. Return result with latency
}

#[tauri::command]
async fn set_default_provider(
    id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    // Implementation using ProviderStore::set_default
}
```

### UI Component Architecture

```
ProviderSettings.tsx (Page)
├── ProviderCard.tsx (per provider)
│   ├── Connection Status Badge
│   ├── Edit Button → ProviderFormDialog
│   ├── Delete Button → Confirm Dialog
│   └── Set Default Button
├── Add Provider Button → ProviderFormDialog
└── ProviderFormDialog.tsx (Add/Edit)
    ├── Provider Type Select
    ├── Display Name Input
    ├── API Key Input (password type)
    ├── Base URL Input
    ├── Default Model Select
    └── Test Connection Button
```

### Provider Presets for UI

Use existing `PROVIDER_PRESETS` from `src/types/config.ts`:

- Cloud providers: anthropic, openai, gemini, deepseek, qwen, moonshot, xai, mistral, groq, openrouter
- Local providers: ollama, lmstudio

Each preset provides:
- `id`, `name`, `type`, `api_key_env`, `base_url`, `models[]`, `category`

### Error Messages (Chinese)

```typescript
const ERROR_MESSAGES = {
  PROVIDER_NOT_FOUND: "未找到提供商配置",
  DUPLICATE_NAME: "提供商名称已存在",
  API_KEY_REQUIRED: "此提供商需要 API 密钥",
  CONNECTION_FAILED: "连接测试失败",
  KEYCHAIN_ERROR: "密钥存储失败",
  VALIDATION_ERROR: "配置验证失败",
  DELETE_FAILED: "删除提供商失败",
};
```

### Testing Standards

Following Stories 3.2-3.5 patterns:

1. **Unit Tests** - Target 20+ tests for components
2. **Integration Tests** - Test Tauri commands with mock keyring
3. **E2E Tests** - Test full provider CRUD flow
4. **Mock keyring** for CI environments without keychain access

### Files to Create

- `apps/omninova-tauri/src/types/provider.ts` - TypeScript types
- `apps/omninova-tauri/src/components/settings/ProviderSettings.tsx` - Settings page
- `apps/omninova-tauri/src/components/settings/ProviderCard.tsx` - Provider card
- `apps/omninova-tauri/src/components/settings/ProviderFormDialog.tsx` - Add/Edit form
- `apps/omninova-tauri/src/hooks/useProviders.ts` - Custom hook for provider operations

### Files to Modify

- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add Tauri commands
- `apps/omninova-tauri/src/types/index.ts` - Export new types (if exists)
- `apps/omninova-tauri/src/pages/Settings.tsx` - Add ProviderSettings route (if exists)

### Files to Reference

- `crates/omninova-core/src/providers/store.rs` - Provider CRUD operations
- `crates/omninova-core/src/providers/config.rs` - Provider types and configs
- `apps/omninova-tauri/src/types/keyring.ts` - Keyring API functions
- `apps/omninova-tauri/src/types/config.ts` - Provider presets
- `apps/omninova-tauri/src/components/Setup/ProviderConfigForm.tsx` - Existing form patterns

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L707-L724] - Story 3.6 requirements
- [Source: crates/omninova-core/src/providers/store.rs] - ProviderStore implementation
- [Source: crates/omninova-core/src/providers/config.rs] - Provider types
- [Source: apps/omninova-tauri/src/types/keyring.ts] - Keyring integration from Story 3.5
- [Source: apps/omninova-tauri/src/types/config.ts#L398-L592] - Provider presets
- [Source: apps/omninova-tauri/src/components/Setup/ProviderConfigForm.tsx] - Existing form patterns

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story file created with comprehensive context from:
- Story 3.5 (Keychain integration) for API key storage
- Existing ProviderStore and ProviderConfig implementations
- Existing ProviderConfigForm for UI patterns
- Provider presets from config.ts

### File List

**To Create:**
- `apps/omninova-tauri/src/types/provider.ts` - TypeScript types
- `apps/omninova-tauri/src/components/settings/ProviderSettings.tsx` - Settings page
- `apps/omninova-tauri/src/components/settings/ProviderCard.tsx` - Provider card
- `apps/omninova-tauri/src/components/settings/ProviderFormDialog.tsx` - Form dialog
- `apps/omninova-tauri/src/hooks/useProviders.ts` - Custom hook

**To Modify:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add Tauri commands
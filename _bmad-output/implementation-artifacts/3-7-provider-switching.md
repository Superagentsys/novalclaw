# Story 3.7: Provider 切换与代理默认提供商

Status: complete

## Story

As a **用户**,
I want **为每个 AI 代理指定默认的 LLM 提供商**,
so that **不同代理可以使用最适合其任务的模型**.

## Acceptance Criteria

1. **AC1: Agent Default Provider** - 可以在代理设置中选择默认提供商
2. **AC2: Auto-use Default Provider** - 对话时自动使用代理的默认提供商
3. **AC3: Runtime Provider Switch** - 可以在对话中临时切换提供商
4. **AC4: Unavailable Provider Handling** - 默认提供商不可用时显示明确的错误并建议切换
5. **AC5: No Restart Required** - 提供商切换不需要重启应用

## Tasks / Subtasks

- [x] Task 1: Agent Model Provider Field (AC: #1)
  - [x] 1.1 Add `default_provider_id` field to `AgentModel` struct in `model.rs`
  - [x] 1.2 Add `default_provider_id` to `NewAgent` struct
  - [x] 1.3 Add `default_provider_id` to `AgentUpdate` struct
  - [x] 1.4 Update database migration to add `default_provider_id` column to `agents` table
  - [x] 1.5 Update `AgentStore` CRUD operations to handle provider field
  - [x] 1.6 Add TypeScript types for `defaultProviderId` in `agent.ts`

- [x] Task 2: Provider Selection UI Components (AC: #1)
  - [x] 2.1 Create `ProviderSelector.tsx` component for selecting providers
  - [x] 2.2 Add provider dropdown with connection status indicators
  - [x] 2.3 Display current provider's model info
  - [x] 2.4 Add "Test Connection" button inline
  - [x] 2.5 Handle empty providers state with link to settings
  - [x] 2.6 Show provider category badges (cloud/local)

- [x] Task 3: Agent Form Provider Integration (AC: #1)
  - [x] 3.1 Integrate `ProviderSelector` into agent creation form
  - [x] 3.2 Integrate `ProviderSelector` into agent edit form
  - [x] 3.3 Add validation for provider existence
  - [x] 3.4 Populate provider dropdown with available providers
  - [x] 3.5 Set default selection to global default provider

- [x] Task 4: Conversation Provider Switching (AC: #2, #3)
  - [x] 4.1 Add `current_provider_id` to conversation/session state
  - [x] 4.2 Create `useConversationProvider` hook for provider management
  - [x] 4.3 Add provider selector to chat interface header
  - [x] 4.4 Implement temporary provider switch during conversation
  - [x] 4.5 Reset to default provider on new conversation
  - [x] 4.6 Persist provider choice per session

- [x] Task 5: Provider Unavailable Handling (AC: #4)
  - [x] 5.1 Create `ProviderUnavailableDialog` component
  - [x] 5.2 Add error detection for unavailable providers
  - [x] 5.3 Show available provider suggestions
  - [x] 5.4 Add "Switch to..." quick action button
  - [x] 5.5 Handle API key missing scenario
  - [x] 5.6 Handle network error scenarios

- [x] Task 6: Tauri Commands for Provider Assignment (AC: #1, #5)
  - [x] 6.1 Add `set_agent_default_provider` Tauri command
  - [x] 6.2 Add `get_agent_provider` Tauri command
  - [x] 6.3 Add `validate_provider_for_agent` Tauri command
  - [x] 6.4 Ensure provider switch works without restart
  - [x] 6.5 Add proper error handling with Chinese messages

- [x] Task 7: State Management Integration (AC: #2, #3, #5)
  - [x] 7.1 Create `providerStore.ts` with Zustand for global provider state
  - [x] 7.2 Add provider switching actions
  - [x] 7.3 Sync provider list with backend on app start
  - [x] 7.4 Add `useAgentProvider` hook combining agent + provider data
  - [x] 7.5 Implement provider change events via Tauri

- [x] Task 8: Unit Tests (All ACs)
  - [x] 8.1 Add unit tests for `ProviderSelector` component
  - [x] 8.2 Add unit tests for `ProviderUnavailableDialog` component
  - [x] 8.3 Add unit tests for `useConversationProvider` hook
  - [x] 8.4 Add unit tests for `useAgentProvider` hook
  - [x] 8.5 Add Rust tests for agent model provider field

## Dev Notes

### Existing Implementation Context

**IMPORTANT:** This story builds on multiple completed implementations:

1. **Story 3.6 (Provider 配置界面)** - Complete provider CRUD UI:
   - `ProviderSettings.tsx` - Settings page with provider list
   - `ProviderCard.tsx` - Individual provider card with status
   - `ProviderFormDialog.tsx` - Add/Edit provider dialog
   - `useProviders.ts` - Provider management hook with connection testing

2. **Agent Model** in `crates/omninova-core/src/agent/model.rs`:
   - `AgentModel` struct - needs `default_provider_id` field
   - `NewAgent` struct - for creating agents
   - `AgentUpdate` struct - for updating agents
   - `AgentStore` in `store.rs` - CRUD operations

3. **Provider System** in `crates/omninova-core/src/providers/`:
   - `ProviderStore` - Provider CRUD with SQLite
   - `ProviderConfig` - Provider configuration struct
   - Connection testing capabilities

4. **Keyring Integration** from Story 3.5:
   - Secure API key storage
   - `apiKeyExists()` - Check if key is stored
   - `getApiKey()` - Retrieve stored key

### Database Schema Changes

Add `default_provider_id` column to `agents` table:

```sql
-- Migration: Add default_provider_id to agents table
ALTER TABLE agents ADD COLUMN default_provider_id TEXT;

-- Create index for provider lookups
CREATE INDEX idx_agents_default_provider ON agents(default_provider_id);
```

### TypeScript Type Definitions

Update `apps/omninova-tauri/src/types/agent.ts`:

```typescript
/**
 * 代理模型（与后端 AgentModel 一致）
 */
export interface AgentModel {
  id: number;
  agent_uuid: string;
  name: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  status: AgentStatus;
  system_prompt?: string;
  default_provider_id?: string;  // NEW: Default provider ID
  created_at: number;
  updated_at: number;
}

/**
 * 新代理数据
 */
export interface NewAgent {
  name: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  system_prompt?: string;
  default_provider_id?: string;  // NEW: Optional default provider
}

/**
 * 代理更新数据
 */
export interface AgentUpdate {
  name?: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  system_prompt?: string;
  status?: AgentStatus;
  default_provider_id?: string;  // NEW: Update provider
}
```

Add new types to `apps/omninova-tauri/src/types/provider.ts`:

```typescript
/**
 * Provider with agent assignment info
 */
export interface ProviderWithAssignment extends ProviderWithStatus {
  assignedAgentsCount: number;
}

/**
 * Agent provider validation result
 */
export interface AgentProviderValidation {
  isValid: boolean;
  errors: string[];
  warnings: string[];
  suggestions?: string[];
}
```

### Tauri Commands Pattern

Follow existing patterns from `lib.rs`:

```rust
#[tauri::command]
async fn set_agent_default_provider(
    agent_id: String,
    provider_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    // 1. Validate provider exists
    // 2. Validate provider has API key (if required)
    // 3. Update agent's default_provider_id
    // 4. Return success
}

#[tauri::command]
async fn get_agent_provider(
    agent_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<ProviderConfig>, String> {
    // 1. Get agent by ID
    // 2. Get provider by agent.default_provider_id
    // 3. Return provider config or None
}

#[tauri::command]
async fn validate_provider_for_agent(
    provider_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<AgentProviderValidation, String> {
    // 1. Check provider exists
    // 2. Check API key exists (if required)
    // 3. Check provider is not deleted
    // 4. Return validation result with suggestions
}
```

### UI Component Architecture

```
AgentFormDialog.tsx (existing)
├── ProviderSelector.tsx (NEW)
│   ├── Provider Dropdown (with status badges)
│   ├── Connection Status Indicator
│   ├── Test Connection Button
│   └── "Manage Providers" Link

ChatInterface.tsx (existing)
├── ChatHeader.tsx (existing)
│   └── ConversationProviderSelector.tsx (NEW)
│       ├── Current Provider Badge
│       ├── Switch Provider Dropdown
│       └── Temporary Switch Indicator

ProviderUnavailableDialog.tsx (NEW)
├── Error Message
├── Available Providers List
├── "Switch to..." Actions
└── "Go to Settings" Link
```

### Provider Selector Component

```tsx
// ProviderSelector.tsx
interface ProviderSelectorProps {
  /** Currently selected provider ID */
  value?: string;
  /** Called when provider selection changes */
  onChange: (providerId: string | undefined) => void;
  /** Show test connection button */
  showTestButton?: boolean;
  /** Show manage providers link when empty */
  showEmptyStateLink?: boolean;
  /** Disabled state */
  disabled?: boolean;
}

// Usage in AgentFormDialog
<ProviderSelector
  value={formData.defaultProviderId}
  onChange={(id) => setFormData({ ...formData, defaultProviderId: id })}
  showTestButton
  showEmptyStateLink
/>
```

### Conversation Provider Hook

```typescript
// useConversationProvider.ts
interface UseConversationProviderReturn {
  /** Current provider for this conversation */
  currentProvider: ProviderWithStatus | null;
  /** Agent's default provider */
  defaultProvider: ProviderWithStatus | null;
  /** Whether using temporary switch */
  isTemporarySwitch: boolean;
  /** Available providers for switching */
  availableProviders: ProviderWithStatus[];
  /** Switch provider temporarily */
  switchProvider: (providerId: string) => Promise<void>;
  /** Reset to default provider */
  resetToDefault: () => void;
  /** Handle provider unavailable error */
  handleProviderError: (error: Error) => void;
}
```

### Error Messages (Chinese)

```typescript
const PROVIDER_ERRORS = {
  PROVIDER_NOT_FOUND: "未找到提供商配置",
  API_KEY_MISSING: "提供商缺少 API 密钥，请先配置",
  PROVIDER_UNAVAILABLE: "提供商不可用，请检查网络连接",
  CONNECTION_FAILED: "连接提供商失败",
  NO_DEFAULT_PROVIDER: "代理未配置默认提供商",
  FALLBACK_SUGGESTION: "建议切换到 {name} 提供商继续对话",
};
```

### State Management with Zustand

```typescript
// stores/providerStore.ts
interface ProviderState {
  providers: ProviderWithStatus[];
  defaultProviderId: string | null;
  isLoading: boolean;
  error: string | null;

  // Actions
  loadProviders: () => Promise<void>;
  setDefaultProvider: (id: string) => Promise<void>;
  refreshProviderStatus: (id: string) => Promise<void>;

  // Computed
  getProviderById: (id: string) => ProviderWithStatus | undefined;
  getDefaultProvider: () => ProviderWithStatus | undefined;
}
```

### Testing Standards

Following Stories 3.1-3.6 patterns:

1. **Unit Tests** - Target 20+ tests for components
2. **Integration Tests** - Test Tauri commands with mock data
3. **E2E Tests** - Test full provider switching flow
4. **Mock providers** for testing without real API calls

### Files to Create

- `apps/omninova-tauri/src/components/agent/ProviderSelector.tsx` - Provider dropdown component
- `apps/omninova-tauri/src/components/chat/ConversationProviderSelector.tsx` - Chat header provider switch
- `apps/omninova-tauri/src/components/chat/ProviderUnavailableDialog.tsx` - Error handling dialog
- `apps/omninova-tauri/src/hooks/useConversationProvider.ts` - Conversation provider hook
- `apps/omninova-tauri/src/hooks/useAgentProvider.ts` - Agent provider hook
- `apps/omninova-tauri/src/stores/providerStore.ts` - Zustand store for providers
- `apps/omninova-tauri/src/test/components/ProviderSelector.test.tsx` - Component tests
- `apps/omninova-tauri/src/test/hooks/useConversationProvider.test.ts` - Hook tests

### Files to Modify

- `crates/omninova-core/src/agent/model.rs` - Add provider field to structs
- `crates/omninova-core/src/agent/store.rs` - Update CRUD for provider field
- `crates/omninova-core/src/db/migrations/` - Add migration for new column
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add Tauri commands
- `apps/omninova-tauri/src/types/agent.ts` - Add TypeScript types
- `apps/omninova-tauri/src/types/provider.ts` - Add new types
- `apps/omninova-tauri/src/components/agent/AgentFormDialog.tsx` - Integrate provider selector

### Files to Reference

- `apps/omninova-tauri/src/components/settings/ProviderSettings.tsx` - Provider list patterns
- `apps/omninova-tauri/src/hooks/useProviders.ts` - Provider hook patterns
- `crates/omninova-core/src/agent/store.rs` - Agent CRUD operations
- `crates/omninova-core/src/providers/store.rs` - Provider store patterns

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L725-L739] - Story 3.7 requirements
- [Source: crates/omninova-core/src/agent/model.rs] - Agent model structs
- [Source: crates/omninova-core/src/agent/store.rs] - Agent store implementation
- [Source: apps/omninova-tauri/src/hooks/useProviders.ts] - Provider hook from Story 3.6
- [Source: apps/omninova-tauri/src/types/provider.ts] - Provider types
- [Source: apps/omninova-tauri/src/types/agent.ts] - Agent types
- [Source: _bmad-output/planning-artifacts/architecture.md#L427-L599] - Naming and structure patterns

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story file created with comprehensive context from:
- Story 3.6 (Provider Configuration UI) for provider components and hooks
- Agent model system for struct modifications
- Architecture patterns for naming conventions and state management
- Provider types for integration points

### File List

**To Create:**
- `apps/omninova-tauri/src/components/agent/ProviderSelector.tsx`
- `apps/omninova-tauri/src/components/chat/ConversationProviderSelector.tsx`
- `apps/omninova-tauri/src/components/chat/ProviderUnavailableDialog.tsx`
- `apps/omninova-tauri/src/hooks/useConversationProvider.ts`
- `apps/omninova-tauri/src/hooks/useAgentProvider.ts`
- `apps/omninova-tauri/src/stores/providerStore.ts`
- `apps/omninova-tauri/src/test/components/ProviderSelector.test.tsx`
- `apps/omninova-tauri/src/test/hooks/useConversationProvider.test.ts`

**To Modify:**
- `crates/omninova-core/src/agent/model.rs`
- `crates/omninova-core/src/agent/store.rs`
- `apps/omninova-tauri/src-tauri/src/lib.rs`
- `apps/omninova-tauri/src/types/agent.ts`
- `apps/omninova-tauri/src/types/provider.ts`
- `apps/omninova-tauri/src/components/agent/AgentFormDialog.tsx`
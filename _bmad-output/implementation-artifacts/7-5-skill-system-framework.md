# Story 7.5: 技能系统框架

Status: done

## Story

As a 开发者,
I want 有一个可扩展的技能系统,
So that AI 代理可以具备各种专门能力.

## Acceptance Criteria

1. **AC1: Skill Trait 定义** - Skill trait 已定义，包含方法：execute, validate, describe
2. **AC2: 技能注册表** - 技能注册表已实现，支持技能的注册、查询和管理
3. **AC3: OpenClaw 兼容** - 支持 OpenClaw 技能格式兼容（ARCH-15）
4. **AC4: 上下文访问** - 技能可以访问代理上下文和记忆
5. **AC5: 结果返回** - 技能执行结果可以返回给代理用于响应

## Tasks / Subtasks

- [x] Task 1: 定义 Skill Trait 和核心数据结构 (AC: #1)
  - [x] 1.1 创建 Skill trait 定义（execute, validate, describe 方法）
  - [x] 1.2 定义 SkillContext 结构体（代理上下文、记忆访问）
  - [x] 1.3 定义 SkillResult 结构体（执行结果、状态、元数据）
  - [x] 1.4 定义 SkillMetadata 结构体（名称、版本、描述、依赖）
  - [x] 1.5 定义 SkillError 错误类型

- [x] Task 2: 实现技能注册表 (AC: #2)
  - [x] 2.1 创建 SkillRegistry 结构体
  - [x] 2.2 实现 register_skill 方法
  - [x] 2.3 实现 unregister_skill 方法
  - [x] 2.4 实现 get_skill 方法
  - [x] 2.5 实现 list_skills 方法
  - [x] 2.6 实现技能依赖检查

- [x] Task 3: 实现 OpenClaw 技能格式兼容 (AC: #3)
  - [x] 3.1 定义 OpenClawSkill 结构体（兼容格式）
  - [x] 3.2 实现 OpenClaw 技能解析器
  - [x] 3.3 实现 OpenClawSkillAdapter 适配器
  - [x] 3.4 支持技能配置文件解析（YAML/JSON）
  - [x] 3.5 测试 OpenClaw 技能加载

- [x] Task 4: 实现技能上下文管理 (AC: #4)
  - [x] 4.1 创建 SkillContext 结构体
  - [x] 4.2 实现代理上下文访问接口
  - [x] 4.3 实现记忆系统访问接口
  - [x] 4.4 实现会话状态访问接口
  - [x] 4.5 实现权限控制和安全边界

- [x] Task 5: 实现技能执行器 (AC: #5)
  - [x] 5.1 创建 SkillExecutor 结构体
  - [x] 5.2 实现异步技能执行
  - [x] 5.3 实现执行超时控制
  - [x] 5.4 实现结果缓存机制
  - [x] 5.5 实现执行日志记录

- [x] Task 6: 添加 Tauri Commands (AC: #1-#5)
  - [x] 6.1 添加 `list_available_skills` 命令
  - [x] 6.2 添加 `get_skill_info` 命令
  - [x] 6.3 添加 `execute_skill` 命令
  - [x] 6.4 添加 `register_custom_skill` 命令
  - [x] 6.5 添加 `validate_skill_config` 命令

- [x] Task 7: 实现前端类型定义和 Hook (AC: #1-#5)
  - [x] 7.1 创建 Skill TypeScript 类型定义
  - [x] 7.2 创建 SkillMetadata TypeScript 类型
  - [x] 7.3 创建 useSkills hook
  - [x] 7.4 创建 useSkillExecution hook

- [x] Task 8: 单元测试 (AC: 全部)
  - [x] 8.1 测试 Skill trait 实现
  - [x] 8.2 测试 SkillRegistry 注册和查询
  - [x] 8.3 测试 OpenClaw 技能适配器
  - [x] 8.4 测试 SkillContext 访问控制
  - [x] 8.5 测试 SkillExecutor 执行流程

## Dev Notes

### 架构上下文

Story 7.5 基于 Epic 2 已完成的代理系统、Epic 5 的记忆系统和 Story 7.4 的隐私设置，为代理添加可扩展的技能系统框架。

**依赖关系：**
- **Epic 2 (已完成)**: AgentModel, AgentStore, AgentService 实现
- **Epic 5 (已完成)**: 三层记忆系统实现
- **Story 7.1-7.4 (已完成)**: 配置与个性化基础功能

**功能需求关联：**
- FR37: 用户可以创建和管理AI代理的技能集
- FR50: 开发者可以创建自定义技能和功能

**架构要求关联：**
- ARCH-15: OpenClaw 生态系统完全兼容（API、数据格式、Skills、Agents）

### 后端数据模型

```rust
// 新增: crates/omninova-core/src/skills/traits.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 技能元数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    /// 技能唯一标识符
    pub id: String,
    /// 技能名称
    pub name: String,
    /// 技能版本
    pub version: String,
    /// 技能描述
    pub description: String,
    /// 技能作者
    #[serde(default)]
    pub author: Option<String>,
    /// 技能标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 技能依赖
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// 是否为内置技能
    #[serde(default)]
    pub is_builtin: bool,
    /// 配置 schema (JSON Schema)
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
}

/// 技能执行上下文
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// 代理 ID
    pub agent_id: String,
    /// 会话 ID
    #[serde(default)]
    pub session_id: Option<String>,
    /// 用户输入
    pub user_input: String,
    /// 对话历史
    #[serde(default)]
    pub conversation_history: Vec<ConversationMessage>,
    /// 记忆访问接口
    pub memory_accessor: MemoryAccessor,
    /// 配置参数
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    /// 额外元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 记忆访问接口
#[derive(Debug, Clone)]
pub struct MemoryAccessor {
    /// 记忆管理器引用
    // 实际实现中会持有对 MemoryManager 的引用
}

impl MemoryAccessor {
    /// 检索相关记忆
    pub async fn retrieve_relevant(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        // 实现记忆检索逻辑
        todo!()
    }

    /// 存储记忆
    pub async fn store_memory(&self, entry: MemoryEntry) -> Result<(), String> {
        // 实现记忆存储逻辑
        todo!()
    }
}

/// 技能执行结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillResult {
    /// 执行是否成功
    pub success: bool,
    /// 结果内容
    #[serde(default)]
    pub content: Option<String>,
    /// 结构化数据
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    /// 错误信息
    #[serde(default)]
    pub error: Option<String>,
    /// 执行耗时（毫秒）
    #[serde(default)]
    pub duration_ms: u64,
    /// 额外元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 技能错误类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SkillError {
    /// 配置错误
    ConfigurationError { message: String },
    /// 执行错误
    ExecutionError { message: String },
    /// 超时错误
    TimeoutError { timeout_ms: u64 },
    /// 权限错误
    PermissionError { required: String },
    /// 依赖错误
    DependencyError { missing: Vec<String> },
    /// 验证错误
    ValidationError { errors: Vec<String> },
}

/// Skill trait - 所有技能必须实现此 trait
#[async_trait]
pub trait Skill: Send + Sync {
    /// 获取技能元数据
    fn metadata(&self) -> &SkillMetadata;

    /// 验证技能配置
    fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError>;

    /// 描述技能功能和用法
    fn describe(&self) -> String {
        let meta = self.metadata();
        format!(
            "## {}\n\n{}\n\n**Version:** {}\n**Tags:** {}",
            meta.name,
            meta.description,
            meta.version,
            meta.tags.join(", ")
        )
    }

    /// 执行技能
    async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError>;
}
```

### 技能注册表实现

```rust
// 新增: crates/omninova-core/src/skills/registry.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use super::traits::{Skill, SkillMetadata, SkillError};

/// 技能注册表
pub struct SkillRegistry {
    /// 已注册的技能
    skills: Arc<RwLock<HashMap<String, Arc<dyn Skill>>>>,
    /// 技能分类索引
    category_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
            category_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册技能
    pub async fn register(&self, skill: Arc<dyn Skill>) -> Result<(), SkillError> {
        let metadata = skill.metadata();
        let id = metadata.id.clone();

        // 检查依赖
        for dep in &metadata.dependencies {
            if !self.has_skill(dep).await {
                return Err(SkillError::DependencyError {
                    missing: vec![dep.clone()],
                });
            }
        }

        let mut skills = self.skills.write().await;
        skills.insert(id.clone(), skill);

        // 更新分类索引
        for tag in &metadata.tags {
            let mut index = self.category_index.write().await;
            index.entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        Ok(())
    }

    /// 注销技能
    pub async fn unregister(&self, skill_id: &str) -> Result<(), SkillError> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.remove(skill_id) {
            let metadata = skill.metadata();
            let mut index = self.category_index.write().await;
            for tag in &metadata.tags {
                if let Some(ids) = index.get_mut(tag) {
                    ids.retain(|id| id != skill_id);
                }
            }
            Ok(())
        } else {
            Err(SkillError::ConfigurationError {
                message: format!("Skill not found: {}", skill_id),
            })
        }
    }

    /// 获取技能
    pub async fn get(&self, skill_id: &str) -> Option<Arc<dyn Skill>> {
        let skills = self.skills.read().await;
        skills.get(skill_id).cloned()
    }

    /// 列出所有技能
    pub async fn list_all(&self) -> Vec<SkillMetadata> {
        let skills = self.skills.read().await;
        skills.values().map(|s| s.metadata().clone()).collect()
    }

    /// 按标签筛选技能
    pub async fn list_by_tag(&self, tag: &str) -> Vec<SkillMetadata> {
        let index = self.category_index.read().await;
        let skills = self.skills.read().await;

        if let Some(ids) = index.get(tag) {
            ids.iter()
                .filter_map(|id| skills.get(id).map(|s| s.metadata().clone()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 检查技能是否存在
    pub async fn has_skill(&self, skill_id: &str) -> bool {
        let skills = self.skills.read().await;
        skills.contains_key(skill_id)
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

### OpenClaw 技能适配器

```rust
// 新增: crates/omninova-core/src/skills/openclaw_adapter.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use super::traits::{Skill, SkillMetadata, SkillContext, SkillResult, SkillError};
use std::collections::HashMap;

/// OpenClaw 技能定义格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawSkillDefinition {
    /// 技能名称
    pub name: String,
    /// 技能描述
    pub description: String,
    /// 技能版本
    #[serde(default = "default_version")]
    pub version: String,
    /// 提示词模板
    pub prompt_template: String,
    /// 输入参数定义
    #[serde(default)]
    pub parameters: Vec<OpenClawParameter>,
    /// 输出格式
    #[serde(default)]
    pub output_format: Option<String>,
    /// 示例
    #[serde(default)]
    pub examples: Vec<String>,
}

fn default_version() -> String { "1.0.0".to_string() }

/// OpenClaw 参数定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// OpenClaw 技能适配器
pub struct OpenClawSkillAdapter {
    definition: OpenClawSkillDefinition,
    metadata: SkillMetadata,
}

impl OpenClawSkillAdapter {
    /// 从 YAML 文件加载技能
    pub fn from_yaml(yaml_content: &str) -> Result<Self, SkillError> {
        let definition: OpenClawSkillDefinition = serde_yaml::from_str(yaml_content)
            .map_err(|e| SkillError::ConfigurationError {
                message: format!("Failed to parse OpenClaw skill: {}", e),
            })?;

        let metadata = SkillMetadata {
            id: format!("openclaw-{}", definition.name.to_lowercase().replace(' ', "-")),
            name: definition.name.clone(),
            version: definition.version.clone(),
            description: definition.description.clone(),
            author: None,
            tags: vec!["openclaw".to_string()],
            dependencies: Vec::new(),
            is_builtin: false,
            config_schema: None,
        };

        Ok(Self { definition, metadata })
    }

    /// 从 JSON 文件加载技能
    pub fn from_json(json_content: &str) -> Result<Self, SkillError> {
        let definition: OpenClawSkillDefinition = serde_json::from_str(json_content)
            .map_err(|e| SkillError::ConfigurationError {
                message: format!("Failed to parse OpenClaw skill: {}", e),
            })?;

        let metadata = SkillMetadata {
            id: format!("openclaw-{}", definition.name.to_lowercase().replace(' ', "-")),
            name: definition.name.clone(),
            version: definition.version.clone(),
            description: definition.description.clone(),
            author: None,
            tags: vec!["openclaw".to_string()],
            dependencies: Vec::new(),
            is_builtin: false,
            config_schema: None,
        };

        Ok(Self { definition, metadata })
    }

    /// 渲染提示词模板
    fn render_prompt(&self, context: &SkillContext) -> String {
        let mut prompt = self.definition.prompt_template.clone();

        // 替换用户输入
        prompt = prompt.replace("{{input}}", &context.user_input);

        // 替换配置参数
        for (key, value) in &context.config {
            let placeholder = format!("{{{{{}}}}}", key);
            if let Some(str_val) = value.as_str() {
                prompt = prompt.replace(&placeholder, str_val);
            }
        }

        prompt
    }
}

#[async_trait]
impl Skill for OpenClawSkillAdapter {
    fn metadata(&self) -> &SkillMetadata {
        &self.metadata
    }

    fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
        // 验证必需参数
        for param in &self.definition.parameters {
            if param.required && !config.contains_key(&param.name) {
                return Err(SkillError::ValidationError {
                    errors: vec![format!("Missing required parameter: {}", param.name)],
                });
            }
        }
        Ok(())
    }

    async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError> {
        let start = std::time::Instant::now();

        // 渲染提示词
        let prompt = self.render_prompt(&context);

        // 对于 OpenClaw 技能，返回渲染后的提示词
        // 实际执行将由 Agent Dispatcher 调用 LLM 完成
        Ok(SkillResult {
            success: true,
            content: Some(prompt),
            data: Some(serde_json::json!({
                "skill_type": "openclaw",
                "skill_name": self.definition.name,
            })),
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
            metadata: HashMap::new(),
        })
    }
}
```

### 技能执行器

```rust
// 新增: crates/omninova-core/src/skills/executor.rs

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use super::traits::{Skill, SkillContext, SkillResult, SkillError};
use super::registry::SkillRegistry;

/// 技能执行器配置
#[derive(Debug, Clone)]
pub struct SkillExecutorConfig {
    /// 默认超时时间（毫秒）
    pub default_timeout_ms: u64,
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存 TTL（秒）
    pub cache_ttl_secs: u64,
}

impl Default for SkillExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30000, // 30 seconds
            enable_cache: true,
            cache_ttl_secs: 300, // 5 minutes
        }
    }
}

/// 技能执行器
pub struct SkillExecutor {
    registry: Arc<SkillRegistry>,
    config: SkillExecutorConfig,
}

impl SkillExecutor {
    pub fn new(registry: Arc<SkillRegistry>, config: Option<SkillExecutorConfig>) -> Self {
        Self {
            registry,
            config: config.unwrap_or_default(),
        }
    }

    /// 执行技能
    pub async fn execute(
        &self,
        skill_id: &str,
        context: SkillContext,
    ) -> Result<SkillResult, SkillError> {
        // 获取技能
        let skill = self.registry.get(skill_id).await
            .ok_or_else(|| SkillError::ConfigurationError {
                message: format!("Skill not found: {}", skill_id),
            })?;

        // 验证配置
        skill.validate(&context.config)?;

        // 执行技能（带超时）
        let timeout_duration = Duration::from_millis(self.config.default_timeout_ms);
        let result = timeout(
            timeout_duration,
            skill.execute(context)
        ).await;

        match result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(SkillError::TimeoutError {
                timeout_ms: self.config.default_timeout_ms,
            }),
        }
    }

    /// 批量执行技能
    pub async fn execute_batch(
        &self,
        skills: Vec<(&str, SkillContext)>,
    ) -> Vec<Result<SkillResult, SkillError>> {
        let mut results = Vec::new();
        for (skill_id, context) in skills {
            results.push(self.execute(skill_id, context).await);
        }
        results
    }
}
```

### 前端类型定义

```typescript
// src/types/skill.ts (新增)

export interface SkillMetadata {
  id: string;
  name: string;
  version: string;
  description: string;
  author?: string;
  tags: string[];
  dependencies: string[];
  isBuiltin: boolean;
  configSchema?: Record<string, unknown>;
}

export interface SkillResult {
  success: boolean;
  content?: string;
  data?: Record<string, unknown>;
  error?: string;
  durationMs: number;
  metadata: Record<string, string>;
}

export interface SkillError {
  type: 'configuration' | 'execution' | 'timeout' | 'permission' | 'dependency' | 'validation';
  message: string;
  details?: Record<string, unknown>;
}

export interface SkillContext {
  agentId: string;
  sessionId?: string;
  userInput: string;
  config: Record<string, unknown>;
}

export const DEFAULT_SKILL_TAGS = [
  'productivity',
  'analysis',
  'creative',
  'automation',
  'integration',
  'openclaw',
] as const;

export type SkillTag = typeof DEFAULT_SKILL_TAGS[number];
```

### 文件结构

```
crates/omninova-core/src/skills/
├── mod.rs                    # 模块入口
├── traits.rs                 # Skill trait 定义
├── registry.rs               # 技能注册表
├── executor.rs               # 技能执行器
├── context.rs                # 技能上下文
├── openclaw_adapter.rs       # OpenClaw 适配器
└── builtin/                  # 内置技能
    ├── mod.rs
    ├── web_search.rs         # 网络搜索技能
    ├── file_ops.rs           # 文件操作技能
    └── code_exec.rs          # 代码执行技能

apps/omninova-tauri/src/
├── types/
│   └── skill.ts              # 新增 - 技能类型定义
├── hooks/
│   └── useSkills.ts          # 新增 - 技能管理 hook
└── components/skills/
    ├── SkillList.tsx         # 新增 - 技能列表组件
    └── SkillCard.tsx         # 新增 - 技能卡片组件

apps/omninova-tauri/src-tauri/src/
└── lib.rs                    # 修改 - 添加技能相关命令

crates/omninova-core/src/
├── lib.rs                    # 修改 - 添加 skills 模块导出
└── agent/
    └── dispatcher.rs         # 修改 - 集成技能执行
```

### 命名约定

遵循 architecture.md 中定义的命名约定：

**Rust:**
- Trait: PascalCase (`Skill`, `SkillExecutor`)
- 结构体: PascalCase (`SkillMetadata`, `SkillResult`, `SkillContext`)
- 字段: snake_case (`skill_id`, `duration_ms`)
- 方法: snake_case (`execute`, `validate`, `describe`)

**TypeScript/React:**
- 接口: PascalCase (`SkillMetadata`, `SkillResult`)
- 属性: camelCase (`isBuiltin`, `durationMs`)
- 组件: PascalCase (`SkillList`, `SkillCard`)

### 与 Provider trait 的关系

**Provider trait** (Epic 3) 和 **Skill trait** 的区别：

| 特性 | Provider trait | Skill trait |
|------|---------------|-------------|
| 目的 | 提供 LLM 推理能力 | 提供专门功能能力 |
| 输入 | 消息列表 | 技能上下文 |
| 输出 | 文本响应 | 结构化结果 |
| 示例 | OpenAI, Anthropic, Ollama | Web Search, File Ops, Code Exec |
| 可扩展性 | 第三方 API 集成 | 自定义功能扩展 |

### 与 Channel trait 的关系

**Channel trait** (Epic 6) 和 **Skill trait** 的协作：

```
Channel (接收消息)
    ↓
Agent Dispatcher (路由消息)
    ↓
Skill Execution (执行技能)
    ↓
Memory (存储上下文)
    ↓
Provider (生成响应)
    ↓
Channel (发送响应)
```

### OpenClaw 兼容性

**ARCH-15 要求**: OpenClaw 生态系统完全兼容

OpenClaw 技能格式示例：
```yaml
name: "Web Search"
description: "Search the web for information"
version: "1.0.0"
prompt_template: |
  Search the web for: {{input}}

  Return the most relevant results.
parameters:
  - name: "max_results"
    type: "integer"
    description: "Maximum number of results"
    required: false
    default: 5
output_format: "json"
examples:
  - "What is the capital of France?"
```

### 测试策略

1. **单元测试**：
   - SkillMetadata 序列化/反序列化
   - SkillRegistry 注册和查询
   - OpenClawSkillAdapter 解析和执行
   - SkillExecutor 超时和错误处理

2. **集成测试**：
   - 技能与记忆系统集成
   - 技能与 Provider 协作
   - 技能执行结果持久化

### Previous Story Intelligence (Story 7.4)

**可复用模式：**
- 数据模型扩展模式（添加新模块）
- Tauri commands 结构（list/get/execute 命令）
- 前端 hook 模式（useSkills）
- 错误处理模式（anyhow + ApiError）

**注意事项：**
- 技能执行需要超时控制
- 技能结果需要结构化处理
- OpenClaw 兼容性需要完整测试
- 技能权限需要安全控制

### References

- [Source: epics.md#Story 7.5] - 原始 story 定义
- [Source: architecture.md#FR37] - 技能管理需求
- [Source: architecture.md#FR50] - 自定义技能创建需求
- [Source: architecture.md#ARCH-15] - OpenClaw 兼容性要求
- [Source: providers/traits.rs] - Provider trait 定义参考
- [Source: channels/traits.rs] - Channel trait 定义参考
- [Source: memory/manager.rs] - MemoryManager 实现参考

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

No blocking issues encountered during implementation.

### Completion Notes List

- Implemented core Skill trait with async execution support
- Created SkillRegistry with dependency checking and tag-based categorization
- Implemented OpenClaw skill format adapter for ARCH-15 compatibility
- Created context module with MemoryAccessor and permission system
- Implemented SkillExecutor with timeout control, caching, and logging
- Added Tauri commands for skill management
- Created TypeScript types and React hooks for frontend integration
- All 65 unit tests passing

### File List

**Backend (Rust):**
- `crates/omninova-core/src/skills/mod.rs` - Module entry point with re-exports
- `crates/omninova-core/src/skills/traits.rs` - Core Skill trait and data structures
- `crates/omninova-core/src/skills/registry.rs` - SkillRegistry implementation
- `crates/omninova-core/src/skills/openclaw.rs` - OpenClaw skill format adapter
- `crates/omninova-core/src/skills/context.rs` - SkillContext, MemoryAccessor, permissions
- `crates/omninova-core/src/skills/error.rs` - SkillError type
- `crates/omninova-core/src/skills/executor.rs` - SkillExecutor with timeout and caching

**Tauri Commands:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Added skill-related Tauri commands

**Frontend (TypeScript/React):**
- `apps/omninova-tauri/src/types/skill.ts` - TypeScript type definitions
- `apps/omninova-tauri/src/hooks/useSkills.ts` - React hooks for skill management
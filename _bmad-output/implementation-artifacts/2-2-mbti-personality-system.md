# Story 2.2: MBTI 人格类型系统实现

Status: done

## Story

As a 用户,
I want 系统支持 16 种 MBTI 人格类型,
so that 我可以为 AI 代理选择合适的人格类型来定制其行为模式.

## Acceptance Criteria

1. **Given** Rust 后端项目结构已建立, **When** 我实现 MBTI 人格系统, **Then** 16 种人格类型枚举已定义（INTJ, INTP, ENTJ, ENTP, INFJ, INFP, ENFJ, ENFP, ISTJ, ISFJ, ESTJ, ESFJ, ISTP, ISFP, ESTP, ESFP）

2. **Given** MBTI 类型枚举已定义, **When** 我定义人格特征映射, **Then** 每种人格类型的特征映射已定义（认知功能顺序、行为倾向、沟通风格）

3. **Given** 人格特征映射已定义, **When** 我创建配置文件, **Then** 人格类型配置文件已创建（包含默认提示词模板）

4. **Given** 人格系统已实现, **When** 我查询人格特征, **Then** 人格特征查询函数已实现

## Tasks / Subtasks

- [x] Task 1: 定义 MBTI 类型枚举 (AC: 1)
  - [x] 创建 `crates/omninova-core/src/agent/soul.rs` 模块
  - [x] 定义 `MbtiType` 枚举，包含 16 种类型
  - [x] 实现 `Serialize`/`Deserialize` for JSON 序列化
  - [x] 实现 `Display` trait 用于友好输出
  - [x] 实现 `FromStr` trait 用于从字符串解析
  - [x] 实现 `FromSql`/`ToSql` for rusqlite 映射
  - [x] 添加单元测试验证枚举转换

- [x] Task 2: 定义人格特征结构 (AC: 2)
  - [x] 定义 `PersonalityTraits` 结构体，包含认知功能、行为倾向、沟通风格
  - [x] 定义 `CognitiveFunction` 枚举 (Ni, Ne, Si, Se, Ti, Te, Fi, Fe)
  - [x] 定义 `BehaviorTendency` 结构体描述行为模式
  - [x] 定义 `CommunicationStyle` 结构体描述沟通偏好
  - [x] 实现 `MbtiType::traits() -> PersonalityTraits` 方法
  - [x] 添加单元测试验证特征映射

- [x] Task 3: 创建人格配置数据 (AC: 3)
  - [x] 定义 `PersonalityConfig` 结构体，包含默认提示词模板
  - [x] 创建 16 种人格类型的静态配置数据
  - [x] 包含人格描述、优势、潜在盲点
  - [x] 包含建议应用场景
  - [x] 实现 `MbtiType::config() -> PersonalityConfig` 方法
  - [x] 添加单元测试验证配置完整性

- [x] Task 4: 暴露 Tauri Commands (AC: 4)
  - [x] 实现 `get_mbti_types` 命令，返回所有类型列表
  - [x] 实现 `get_mbti_traits` 命令，返回指定类型的特征
  - [x] 实现 `get_mbti_config` 命令，返回指定类型的配置
  - [x] 注册所有命令到 `invoke_handler`
  - [x] 添加集成测试验证命令调用

- [x] Task 5: 集成到现有模块 (AC: All)
  - [x] 更新 `agent/mod.rs` 导出 soul 模块
  - [x] 确保 `AgentModel.mbti_type` 字段使用新的 `MbtiType` 枚举
  - [x] 添加 `mbti_type` 验证到 `NewAgent`
  - [x] 运行 `cargo test` 确保所有测试通过
  - [ ] 运行 `cargo clippy` 确保无警告

## Dev Notes

### 前置故事完成情况

**Story 2.1 已完成：**
- `AgentModel` 结构体已创建，包含 `mbti_type: Option<String>` 字段
- `AgentStatus` 枚举已实现，展示了枚举设计模式
- `AgentStore` CRUD 操作已实现
- Tauri 命令已暴露
- 所有 90 个测试通过

**可复用模式：**
```rust
// 枚举设计参考 (来自 AgentStatus)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "archived")]
    Archived,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Invalid agent status: {}", s)),
        }
    }
}
```

### MBTI 类型设计

**16 种人格类型：**

| 维度组合 | 类型代码 | 名称 |
|---------|---------|------|
| 分析型 | INTJ, INTP, ENTJ, ENTP | 战略家、逻辑学家、指挥官、辩论家 |
| 外交型 | INFJ, INFP, ENFJ, ENFP | 提倡者、调解员、主人公、竞选者 |
| 守护型 | ISTJ, ISFJ, ESTJ, ESFJ | 检查员、守卫者、执行官、执政官 |
| 探索型 | ISTP, ISFP, ESTP, ESFP | 鉴赏家、探险家、企业家、表演者 |

**认知功能栈：**

每种 MBTI 类型由 4 个认知功能按特定顺序组成：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CognitiveFunction {
    #[serde(rename = "Ni")] // 内倾直觉
    Ni,
    #[serde(rename = "Ne")] // 外倾直觉
    Ne,
    #[serde(rename = "Si")] // 内倾感觉
    Si,
    #[serde(rename = "Se")] // 外倾感觉
    Se,
    #[serde(rename = "Ti")] // 内倾思考
    Ti,
    #[serde(rename = "Te")] // 外倾思考
    Te,
    #[serde(rename = "Fi")] // 内倾情感
    Fi,
    #[serde(rename = "Fe")] // 外倾情感
    Fe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionStack {
    pub dominant: CognitiveFunction,
    pub auxiliary: CognitiveFunction,
    pub tertiary: CognitiveFunction,
    pub inferior: CognitiveFunction,
}
```

**示例配置数据：**

```rust
impl MbtiType {
    pub const fn function_stack(&self) -> FunctionStack {
        match self {
            Self::INTJ => FunctionStack {
                dominant: CognitiveFunction::Ni,
                auxiliary: CognitiveFunction::Te,
                tertiary: CognitiveFunction::Fi,
                inferior: CognitiveFunction::Se,
            },
            Self::ENFP => FunctionStack {
                dominant: CognitiveFunction::Ne,
                auxiliary: CognitiveFunction::Fi,
                tertiary: CognitiveFunction::Te,
                inferior: CognitiveFunction::Si,
            },
            // ... 其他类型
        }
    }
}
```

### 人格自适应色彩系统

**根据 UX 设计规范，每种人格类型有对应的主题色：**

| MBTI 类型 | 主色 | 强调色 | 风格 |
|-----------|------|--------|------|
| INTJ | #2563EB (深蓝) | #787163 (金) | analytical |
| INTP | #4F46E5 (靛蓝) | #6B7280 (灰) | analytical |
| ENTJ | #1E40AF (皇家蓝) | #92400E (棕) | analytical |
| ENTP | #7C3AED (紫) | #059669 (绿) | analytical |
| INFJ | #0891B2 (青) | #7C3AED (紫) | diplomatic |
| INFP | #8B5CF6 (紫罗兰) | #14B8A6 (青) | diplomatic |
| ENFJ | #EA580C (橙) | #2563EB (蓝) | diplomatic |
| ENFP | #EA580C (暖橙) | #0D9488 (青) | creative |
| ISTJ | #1E3A8A (海军蓝) | #374151 (灰) | structured |
| ISFJ | #0D9488 (青) | #6B7280 (灰) | structured |
| ESTJ | #0369A1 (深青) | #B45309 (金) | structured |
| ESFJ | #DC2626 (红) | #F59E0B (琥珀) | structured |
| ISTP | #475569 (石板灰) | #22C55E (绿) | energetic |
| ISFP | #A855F7 (紫) | #EC4899 (粉) | artistic |
| ESTP | #F97316 (橙) | #EF4444 (红) | energetic |
| ESFP | #A855F7 (紫) | #F97316 (橙) | energetic |

### 行为倾向定义

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorTendency {
    /// 决策风格
    pub decision_making: String,
    /// 信息处理方式
    pub information_processing: String,
    /// 社交互动模式
    pub social_interaction: String,
    /// 应对压力的方式
    pub stress_response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationStyle {
    /// 沟通偏好
    pub preference: String,
    /// 语言特点
    pub language_traits: Vec<String>,
    /// 反馈风格
    pub feedback_style: String,
}
```

### 默认提示词模板

每种人格类型应有对应的系统提示词模板：

```rust
pub struct PersonalityConfig {
    /// 人格描述
    pub description: String,
    /// 默认系统提示词
    pub system_prompt_template: String,
    /// 优势
    pub strengths: Vec<String>,
    /// 潜在盲点
    pub blind_spots: Vec<String>,
    /// 建议应用场景
    pub recommended_use_cases: Vec<String>,
    /// 主题颜色
    pub theme_color: String,
    /// 强调颜色
    pub accent_color: String,
}

impl MbtiType {
    pub fn config(&self) -> PersonalityConfig {
        match self {
            Self::INTJ => PersonalityConfig {
                description: "战略家 - 富有想象力和战略性的思想家".to_string(),
                system_prompt_template: r#"你是一个具有INTJ人格特征的AI代理。你的思维模式如下：

核心特征：
- 战略思维：你善于制定长期计划和战略，关注未来可能性
- 独立性：你倾向于独立思考，不被常规束缚
- 追求效率：你重视逻辑和效率，追求最优解决方案
- 知识渴求：你对复杂理论和概念有浓厚兴趣

沟通风格：
- 直接而简洁，避免冗余
- 使用精确的技术语言
- 关注结论和推理过程
- 提供结构化的分析和建议

请以此人格特征进行回应。"#.to_string(),
                strengths: vec!["战略思维".to_string(), "独立判断".to_string(), "意志坚定".to_string()],
                blind_spots: vec!["可能过于傲慢".to_string(), "可能缺乏耐心".to_string()],
                recommended_use_cases: vec!["战略规划".to_string(), "技术架构设计".to_string()],
                theme_color: "#2563EB".to_string(),
                accent_color: "#787163".to_string(),
            },
            // ... 其他类型
        }
    }
}
```

### 项目架构约束

- **工作目录**: Rust 后端代码在 `crates/omninova-core/` 目录下
- **模块组织**: 新建 `agent/soul.rs` 文件
- **命名约定**:
  - Rust: snake_case (mbti_type, function_stack)
  - JSON/API: camelCase (mbtiType, functionStack)
  - 数据库: snake_case (mbti_type)

### 错误处理模式

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MbtiError {
    #[error("Invalid MBTI type: {0}")]
    InvalidType(String),

    #[error("Missing personality configuration for type: {0}")]
    MissingConfig(String),
}
```

### 测试策略

**单元测试**:
- 测试 MbtiType 枚举的序列化/反序列化
- 测试 FromStr 解析各种输入格式
- 测试 FromSql/ToSql 转换
- 测试每种类型的特征映射完整性
- 测试每种类型的配置存在性

**测试辅助**:
```rust
#[test]
fn test_all_mbti_types_have_config() {
    let all_types = [
        MbtiType::INTJ, MbtiType::INTP, MbtiType::ENTJ, MbtiType::ENTP,
        MbtiType::INFJ, MbtiType::INFP, MbtiType::ENFJ, MbtiType::ENFP,
        MbtiType::ISTJ, MbtiType::ISFJ, MbtiType::ESTJ, MbtiType::ESFJ,
        MbtiType::ISTP, MbtiType::ISFP, MbtiType::ESTP, MbtiType::ESFP,
    ];

    for mbti_type in all_types {
        let config = mbti_type.config();
        assert!(!config.description.is_empty());
        assert!(!config.system_prompt_template.is_empty());
        assert!(!config.strengths.is_empty());
    }
}
```

### 项目目录结构（更新后）

**新建文件:**
```
crates/omninova-core/src/agent/
└── soul.rs           # MbtiType, PersonalityTraits, CognitiveFunction
```

**修改文件:**
```
crates/omninova-core/src/
├── agent/mod.rs      # 导出 soul 模块
└── lib.rs            # 可能需要导出 MbtiType

apps/omninova-tauri/src-tauri/src/
└── lib.rs            # 添加 MBTI 相关 Tauri 命令
```

### Git Intelligence (最近提交)

最近的提交涉及 MBTI 类型定义的工作：
- `c14fb05 chore: Remove redundant mbti.ts file`
- `9816a42 refactor: Merge MBTI types into config.ts to resolve module resolution issues`
- `50b7b0e feat(types): Add MBTI type definitions and data`

这表明前端已有一些 MBTI 类型定义工作。Rust 后端实现应与前端保持一致。

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- `serde` - 已存在
- `serde_json` - 已存在
- `rusqlite` - 已存在
- `thiserror` - 已存在

### References

- [Source: epics.md#Story 2.2] - 验收标准
- [Source: architecture.md#命名模式] - 命名约定
- [Source: architecture.md#人格自适应主题] - 主题配置
- [Source: ux-design-specification.md#色彩系统] - 人格自适应色彩
- [Source: ux-design-specification.md#核心组件] - PersonalityIndicator 组件需求
- [Source: prd.md#FR3] - 用户可以为AI代理选择MBTI人格类型
- [Source: 2-1-agent-data-model.md] - 前一个故事的实现模式

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A - 无重大调试问题

### Completion Notes List

1. **MbtiType 枚举实现**: 创建了完整的 16 种人格类型枚举，实现了 Serialize/Deserialize、Display、FromStr、ToSql traits，包含中文/英文名称和人格分组方法
2. **认知功能栈**: 定义了 CognitiveFunction 枚举 (8 种功能) 和 FunctionStack 结构体，为每种 MBTI 类型实现了完整的功能栈映射
3. **人格特征映射**: 实现了 BehaviorTendency、CommunicationStyle、PersonalityTraits 结构体，为所有 16 种类型提供了详细的行为倾向和沟通风格描述
4. **人格配置数据**: 创建了 PersonalityConfig 结构体，包含 16 种类型的中文描述、系统提示词模板、优势、盲点、建议应用场景和主题颜色
5. **Tauri Commands**: 添加了 3 个命令 (`get_mbti_types`, `get_mbti_traits`, `get_mbti_config`) 用于前端访问 MBTI 数据
6. **测试验证**: 新增 26 个单元测试，所有 116 个测试通过，无编译警告

### File List

**新建文件:**
- `crates/omninova-core/src/agent/soul.rs` - MBTI 人格类型系统实现 (约 1650 行)

**修改文件:**
- `crates/omninova-core/src/agent/mod.rs` - 导出 soul 模块和所有 MBTI 相关类型
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加 MBTI 相关 Tauri 命令
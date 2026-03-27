# Story 9.2: 代理性能监控

**Story ID:** 9.2
**Status:** ready-for-dev
**Created:** 2026-03-26
**Epic:** Epic 9 - 系统监控与管理

---

## 用户故事

**As a** 用户,
**I want** 监控 AI 代理的性能指标,
**So that** 我可以了解代理的响应效率和可靠性.

---

## 验收标准

### 功能验收标准

1. **Given** AI 代理已配置, **When** 我查看代理性能监控, **Then** 显示每个代理的平均响应时间
2. **Given** AI 代理已配置, **When** 我查看代理性能监控, **Then** 显示请求成功率统计
3. **Given** AI 代理已配置, **When** 我查看代理性能监控, **Then** 显示按时间段划分的性能趋势
4. **Given** AI 代理已配置, **When** 我查看代理性能监控, **Then** 显示每个提供商的响应时间对比
5. **Given** 响应时间超过阈值, **When** 我查看代理性能监控, **Then** 响应时间超过阈值时高亮显示
6. **Given** AI 代理已配置, **When** 我查看代理性能监控, **Then** 支持按代理、时间范围筛选数据

### 非功能验收标准

- 性能数据查询响应时间 < 200ms
- 支持最近 24 小时的详细数据，超过 24 小时后聚合为小时级数据
- 内存占用增量 < 10MB

---

## 技术需求

### 后端实现 (Rust)

#### 1. 性能指标数据结构

**位置:** `crates/omninova-core/src/observability/agent_metrics.rs`

```rust
/// 单次请求的性能记录
pub struct AgentRequestMetric {
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub timestamp: i64,        // Unix timestamp
    pub response_time_ms: u64,
    pub success: bool,
    pub error_code: Option<String>,
    pub tokens_input: Option<u64>,
    pub tokens_output: Option<u64>,
}

/// 代理聚合性能统计
pub struct AgentPerformanceStats {
    pub agent_id: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_response_time_ms: f64,
    pub min_response_time_ms: u64,
    pub max_response_time_ms: u64,
    pub success_rate: f64,
    pub p50_response_time_ms: u64,
    pub p95_response_time_ms: u64,
    pub p99_response_time_ms: u64,
}

/// 提供商性能统计
pub struct ProviderPerformanceStats {
    pub provider: String,
    pub total_requests: u64,
    pub avg_response_time_ms: f64,
    pub success_rate: f64,
}

/// 时间序列数据点
pub struct MetricDataPoint {
    pub timestamp: i64,
    pub value: f64,
}
```

#### 2. 性能监控服务

**位置:** `crates/omninova-core/src/observability/agent_metrics.rs`

```rust
/// 代理性能监控服务
pub struct AgentMetricsMonitor {
    /// 内存中的最近请求记录 (最近 24 小时)
    recent_metrics: Arc<RwLock<Vec<AgentRequestMetric>>>,
    /// 聚合统计数据缓存
    stats_cache: Arc<RwLock<HashMap<String, AgentPerformanceStats>>>,
    /// 数据保留配置
    retention_hours: u64,
}

impl AgentMetricsMonitor {
    /// 记录一次请求
    pub async fn record_request(&self, metric: AgentRequestMetric);

    /// 获取代理性能统计
    pub async fn get_agent_stats(&self, agent_id: &str, time_range: TimeRange) -> AgentPerformanceStats;

    /// 获取所有代理统计
    pub async fn get_all_agent_stats(&self, time_range: TimeRange) -> Vec<AgentPerformanceStats>;

    /// 获取提供商性能对比
    pub async fn get_provider_comparison(&self, time_range: TimeRange) -> Vec<ProviderPerformanceStats>;

    /// 获取时间序列数据
    pub async fn get_time_series(&self, agent_id: &str, metric_type: MetricType, time_range: TimeRange) -> Vec<MetricDataPoint>;

    /// 清理过期数据
    pub async fn prune_expired_data(&self);
}
```

#### 3. 数据存储策略

- **实时数据 (最近 1 小时):** 内存 Ring Buffer，保留每条记录
- **详细数据 (1-24 小时):** 内存 + 按分钟聚合
- **历史数据 (> 24 小时):** SQLite 持久化 + 按小时聚合 (可选，Phase 2)

#### 4. Gateway API 端点

**位置:** `crates/omninova-core/src/gateway/mod.rs` (添加新路由)

```rust
// 新增 API 端点
GET /api/v1/metrics/agents                    // 获取所有代理性能统计
GET /api/v1/metrics/agents/{id}               // 获取单个代理性能统计
GET /api/v1/metrics/providers                 // 获取提供商性能对比
GET /api/v1/metrics/agents/{id}/timeseries    // 获取时间序列数据

// 查询参数
?from=<timestamp>&to=<timestamp>              // 时间范围
&interval=<1m|5m|1h>                          // 聚合间隔
```

#### 5. 与现有系统集成

**位置:** `crates/omninova-core/src/agent/dispatcher.rs`

在 `Agent::process_message` 方法中添加性能指标收集:

```rust
impl Agent {
    pub async fn process_message(&mut self, message: &str) -> Result<String> {
        let start_time = Instant::now();
        let provider_name = self.provider.name().to_string();

        let result = self.process_message_inner(message).await;

        let response_time_ms = start_time.elapsed().as_millis() as u64;
        let success = result.is_ok();

        // 记录性能指标
        if let Some(monitor) = &self.metrics_monitor {
            monitor.record_request(AgentRequestMetric {
                agent_id: self.config.id.clone(),
                provider: provider_name,
                model: self.config.model.clone(),
                timestamp: chrono::Utc::now().timestamp(),
                response_time_ms,
                success,
                error_code: result.as_ref().err().map(|e| e.to_string()),
                tokens_input: None,  // 可从 provider 响应中提取
                tokens_output: None,
            }).await;
        }

        result
    }
}
```

### 前端实现 (React + TypeScript)

#### 1. 类型定义

**位置:** `apps/omninova-tauri/src/types/metrics.ts`

```typescript
export interface AgentPerformanceStats {
  agentId: string;
  agentName: string;
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  avgResponseTimeMs: number;
  minResponseTimeMs: number;
  maxResponseTimeMs: number;
  successRate: number;
  p50ResponseTimeMs: number;
  p95ResponseTimeMs: number;
  p99ResponseTimeMs: number;
}

export interface ProviderPerformanceStats {
  provider: string;
  totalRequests: number;
  avgResponseTimeMs: number;
  successRate: number;
}

export interface MetricDataPoint {
  timestamp: number;
  value: number;
}

export interface TimeRange {
  from: number;  // Unix timestamp
  to: number;
}
```

#### 2. 状态管理

**位置:** `apps/omninova-tauri/src/stores/metricsStore.ts`

```typescript
interface MetricsStore {
  agentStats: AgentPerformanceStats[];
  providerStats: ProviderPerformanceStats[];
  timeSeries: MetricDataPoint[];
  isLoading: boolean;
  error: string | null;
  timeRange: TimeRange;

  fetchAgentStats: (timeRange?: TimeRange) => Promise<void>;
  fetchProviderStats: (timeRange?: TimeRange) => Promise<void>;
  fetchTimeSeries: (agentId: string, timeRange?: TimeRange) => Promise<void>;
  setTimeRange: (range: TimeRange) => void;
}
```

#### 3. 组件结构

**位置:** `apps/omninova-tauri/src/components/metrics/`

```
metrics/
├── AgentPerformancePanel.tsx    # 主面板组件
├── AgentStatsTable.tsx          # 代理统计表格
├── ProviderComparison.tsx       # 提供商对比图表
├── PerformanceChart.tsx         # 时间序列图表
├── MetricCard.tsx               # 单个指标卡片
└── TimeRangeSelector.tsx        # 时间范围选择器
```

#### 4. UI 设计要点

- 使用 Shadcn/UI 的 `Card`, `Table`, `Tabs` 组件
- 图表使用轻量级库或 Canvas 绘制简单折线图
- 响应时间超过阈值 (如 3000ms) 时使用警告色高亮
- 成功率低于 95% 时显示警告图标

---

## 架构合规要求

### 命名约定

| 层级 | 规则 | 示例 |
|------|------|------|
| Rust 结构体 | PascalCase | `AgentPerformanceStats` |
| Rust 函数 | snake_case | `get_agent_stats()` |
| TypeScript 类型 | PascalCase | `AgentPerformanceStats` |
| TypeScript 函数 | camelCase | `fetchAgentStats()` |
| API 端点 | kebab-case | `/api/v1/metrics/agents` |

### 文件组织

```
crates/omninova-core/src/observability/
├── mod.rs              # 更新导出
├── monitor.rs          # 现有系统监控
├── agent_metrics.rs    # 新增: 代理性能监控
└── prometheus.rs       # 现有 Prometheus 指标

apps/omninova-tauri/src/
├── types/metrics.ts    # 新增: 性能指标类型
├── stores/metricsStore.ts  # 新增: 性能指标状态
└── components/metrics/     # 新增: 性能监控组件
```

### API 响应格式

遵循现有 `ApiResponse<T>` 包装模式:

```rust
pub struct MetricsApiResponse<T> {
    pub data: T,
    pub timestamp: i64,
}
```

---

## 测试要求

### Rust 单元测试

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_record_request() { /* ... */ }

    #[test]
    fn test_get_agent_stats() { /* ... */ }

    #[test]
    fn test_percentile_calculation() { /* ... */ }

    #[test]
    fn test_data_pruning() { /* ... */ }
}
```

### 前端测试

- 组件渲染测试
- API 调用 mock 测试
- 时间范围计算测试

---

## 依赖关系

### 前置依赖

- ✅ Story 9-1 系统资源监控 (已完成后端)
- ✅ Agent 系统和 Dispatcher
- ✅ Gateway HTTP 服务

### 后置依赖

- Story 9-3 系统通知管理 (可选关联)
- Story 9-5 运行模式管理 (可选关联)

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 性能数据占用过多内存 | 中 | 实现数据聚合和过期清理机制 |
| 高并发写入竞争 | 低 | 使用无锁数据结构或批量写入 |
| 前端图表性能 | 低 | 限制数据点数量，使用虚拟滚动 |

---

## 实施建议

### 推荐实施顺序

1. **后端核心模块** (agent_metrics.rs)
   - 数据结构定义
   - AgentMetricsMonitor 实现
   - 单元测试

2. **Gateway 集成**
   - 添加 API 路由
   - 与 Dispatcher 集成收集指标

3. **前端组件**
   - 类型定义和状态管理
   - 基础 UI 组件
   - 图表组件

4. **测试与优化**
   - 性能测试
   - 内存使用优化

### 参考现有实现

- `observability/monitor.rs` - 系统监控模式参考
- `gateway/mod.rs` - API 端点模式参考
- `agent/dispatcher.rs` - 请求处理流程

---

## 完成标准

- [ ] 后端性能指标收集服务实现完成
- [ ] Gateway API 端点可用并返回正确数据
- [ ] 前端组件可显示代理性能统计
- [ ] 时间范围筛选功能正常
- [ ] 提供商对比视图可用
- [ ] 单元测试覆盖核心逻辑
- [ ] 更新 sprint-status.yaml 状态为 done
# OmniNoval Grafana 面板

## 前置条件

1. 在 `config.toml` 或桌面「设置 → 通用」中开启 **Prometheus 指标**。
2. 启动网关后，指标暴露在独立端口（默认 `http://127.0.0.1:9090/metrics`）。
3. Prometheus 抓取示例：

```yaml
scrape_configs:
  - job_name: omninova
    static_configs:
      - targets: ["127.0.0.1:9090"]
```

## 导入面板

1. Grafana → **Dashboards** → **Import**
2. 上传 `omninova-dashboard.json` 或粘贴 JSON
3. 选择 Prometheus 数据源（变量 `DS_PROMETHEUS`）

## 主要指标

| 指标 | 说明 |
|------|------|
| `omninova_inbound_requests_total` | 入站请求数（按 channel） |
| `omninova_inbound_errors_total` | 入站错误（按 stage） |
| `omninova_inbound_duration_seconds` | 请求延迟直方图 |
| `omninova_tool_calls_total` | 工具调用 |
| `omninova_provider_calls_total` | LLM 调用 |
| `omninova_audit_events_total` | 审计事件 |
| `omninova_approval_events_total` | 审批事件 |
| `omninova_estop_events_total` | 急停事件 |
| `omninova_active_sessions` | 活跃会话数 |

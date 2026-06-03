import { useCallback, useEffect, useMemo, useState } from "react";
import { invokeTauri, isTauriEnvironment } from "../../utils/tauri";
import type { EmployeeManifest, EmployeeSummary } from "../../types/config";

interface EmployeesProps {
  /** 点击「会话」时回调，由上层切换到聊天并注入该员工上下文 */
  onOpenSession?: (employee: { id: string; name: string }) => void;
}

interface EditorState {
  id: string;
  name: string;
  description: string;
  prompt: string;
  type: string;
  mcpText: string;
}

const EMPTY_EDITOR: EditorState = {
  id: "",
  name: "",
  description: "",
  prompt: "",
  type: "",
  mcpText: "",
};

const PRESETS: Array<{ name: string; type: string; description: string; prompt: string }> = [
  {
    name: "SRE 运维专家",
    type: "运维",
    description: "专注监控分析、告警处置、K8s 故障排查",
    prompt:
      "你是一名资深 SRE 运维专家。聚焦稳定性、监控告警分析、Kubernetes 与容器故障排查。回答克制、专业、给出可执行的排查步骤与命令；遇到高风险操作先确认。只做运维分内之事，不越界。",
  },
  {
    name: "DevOps 流水线顾问",
    type: "运维",
    description: "专注 CI/CD、Ansible、Terraform 自动化",
    prompt:
      "你是一名 DevOps 顾问。聚焦 CI/CD 流水线、基础设施即代码（Terraform）、配置管理（Ansible）。给出规范、可复用的流水线与脚本建议，强调幂等与回滚。",
  },
  {
    name: "数据库 DBA",
    type: "运维",
    description: "专注数据库性能、慢查询、备份恢复",
    prompt:
      "你是一名数据库管理专家（DBA）。聚焦慢查询优化、索引设计、主从与备份恢复。先看执行计划与指标再给结论，谨慎对待任何写操作与变更。",
  },
];

export function Employees({ onOpenSession }: EmployeesProps) {
  const [list, setList] = useState<EmployeeSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauriEnvironment()) {
      setError("数字员工管理需在桌面应用中打开");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const items = await invokeTauri<EmployeeSummary[]>("list_employees");
      setList(items);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const startCreate = (preset?: (typeof PRESETS)[number]) =>
    setEditor(
      preset
        ? { ...EMPTY_EDITOR, name: preset.name, type: preset.type, description: preset.description, prompt: preset.prompt }
        : { ...EMPTY_EDITOR }
    );

  const startEdit = (emp: EmployeeSummary) =>
    setEditor({
      id: emp.id,
      name: emp.name,
      description: emp.description,
      prompt: emp.prompt,
      type: emp.type === "其它" ? "" : emp.type,
      mcpText: "",
    });

  const handleSave = async () => {
    if (!editor) return;
    if (!editor.name.trim()) {
      setError("请填写名称");
      return;
    }
    let mcpServers: unknown = {};
    if (editor.mcpText.trim()) {
      try {
        mcpServers = JSON.parse(editor.mcpText);
      } catch {
        setError("MCP 配置不是合法 JSON");
        return;
      }
    }
    const manifest: EmployeeManifest = {
      id: editor.id,
      name: editor.name.trim(),
      description: editor.description.trim(),
      prompt: editor.prompt,
      enabled: true,
      type: editor.type.trim(),
      mcp_servers: mcpServers,
    };
    setError(null);
    try {
      await invokeTauri<EmployeeManifest>("save_employee", { manifest });
      setEditor(null);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleToggle = async (emp: EmployeeSummary) => {
    try {
      await invokeTauri<EmployeeManifest>("set_employee_enabled", {
        id: emp.id,
        enabled: !emp.enabled,
      });
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async (emp: EmployeeSummary) => {
    if (!window.confirm(`确定删除数字员工「${emp.name}」？将同时删除其专属技能。`)) return;
    try {
      await invokeTauri<boolean>("delete_employee", { id: emp.id });
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const grouped = useMemo(() => {
    const map = new Map<string, EmployeeSummary[]>();
    for (const emp of list) {
      const key = emp.type || "其它";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(emp);
    }
    return [...map.entries()];
  }, [list]);

  return (
    <div className="setup-embed setup-embed--full">
      <header className="setup-embed-head">
        <h1>数字员工</h1>
        <p className="setup-embed-sub">
          为不同垂直场景（如 <b>SRE 运维专家</b>、<b>DevOps 顾问</b>、<b>DBA</b>）创建专属对话角色，
          每个员工有独立人设、精简技能与专属 MCP（运维场景如 Prometheus / K8s）。
          点击「会话」即以该角色开启聚焦、专业的对话，降低 token 消耗与技能干扰。
        </p>
      </header>

      <section className="setup-section">
        <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap", marginBottom: "0.75rem" }}>
          <button type="button" className="setup-primary-btn" onClick={() => startCreate()} disabled={loading}>
            添加数字员工
          </button>
          <button type="button" className="setup-secondary-btn" onClick={() => void refresh()} disabled={loading}>
            刷新
          </button>
        </div>
        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          <span className="setup-embed-sub" style={{ alignSelf: "center", margin: 0 }}>快速模板：</span>
          {PRESETS.map((p) => (
            <button key={p.name} type="button" className="emp-preset-pill" onClick={() => startCreate(p)}>
              {p.name}
            </button>
          ))}
        </div>
        {error && <p style={{ color: "#f87171", fontSize: "0.9rem", marginTop: "0.75rem" }}>{error}</p>}
      </section>

      {editor && (
        <section className="setup-section">
          <h2>{editor.id ? `修改：${editor.name}` : "新增数字员工"}</h2>
          <div className="setup-grid" style={{ gridTemplateColumns: "1fr" }}>
            <label>
              名称{editor.id ? "（不可修改）" : ""}
              <input
                value={editor.name}
                disabled={!!editor.id}
                placeholder="如：SRE 运维专家"
                onChange={(e) => setEditor({ ...editor, name: e.target.value })}
              />
            </label>
            <label>
              类型 / 分类
              <input
                value={editor.type}
                placeholder="如：运维（留空为「其它」）"
                onChange={(e) => setEditor({ ...editor, type: e.target.value })}
              />
            </label>
            <label>
              描述
              <input
                value={editor.description}
                placeholder="该角色的职责说明"
                onChange={(e) => setEditor({ ...editor, description: e.target.value })}
              />
            </label>
            <label>
              人设 / 系统提示（Prompt）
              <textarea
                rows={5}
                value={editor.prompt}
                placeholder="定义该角色的语气、风格、职责边界…"
                onChange={(e) => setEditor({ ...editor, prompt: e.target.value })}
              />
            </label>
            <label>
              专属 MCP 配置（JSON，选填；当前版本存储保留，运行时尚未执行）
              <textarea
                rows={5}
                value={editor.mcpText}
                placeholder={'{\n  "prometheus": { "command": "npx", "args": ["prometheus-mcp@latest","stdio"], "env": {"PROMETHEUS_URL":"http://localhost:9090"} }\n}'}
                onChange={(e) => setEditor({ ...editor, mcpText: e.target.value })}
                style={{ fontFamily: "var(--font-mono)", fontSize: "12px" }}
              />
            </label>
          </div>
          <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem" }}>
            <button type="button" className="setup-primary-btn" onClick={() => void handleSave()}>
              保存
            </button>
            <button type="button" className="setup-secondary-btn" onClick={() => setEditor(null)}>
              取消
            </button>
          </div>
        </section>
      )}

      <section className="setup-section">
        <h2>员工列表</h2>
        {list.length === 0 ? (
          <p className="setup-embed-sub" style={{ marginTop: 0 }}>暂无数字员工，点击「添加数字员工」或选择快速模板创建。</p>
        ) : (
          grouped.map(([type, emps]) => (
            <div key={type} style={{ marginBottom: "1rem" }}>
              <div className="emp-group-title">{type}</div>
              <div className="setup-grid" style={{ gridTemplateColumns: "1fr" }}>
                {emps.map((emp) => (
                  <article key={emp.id} className={`emp-card${emp.enabled ? "" : " emp-card--disabled"}`}>
                    <div className="emp-card-main">
                      <div className="emp-card-title">
                        <strong>{emp.name}</strong>
                        {!emp.enabled && <span className="emp-badge">已禁用</span>}
                      </div>
                      {emp.description && <p className="emp-card-desc">{emp.description}</p>}
                      <p className="emp-card-meta">
                        技能 {emp.skill_names.length} · MCP {emp.mcp_server_keys.length}
                        {emp.skill_names.length ? ` · ${emp.skill_names.join("、")}` : ""}
                      </p>
                    </div>
                    <div className="emp-card-actions">
                      <button
                        type="button"
                        className="setup-primary-btn"
                        disabled={!emp.enabled}
                        onClick={() => onOpenSession?.({ id: emp.id, name: emp.name })}
                      >
                        会话
                      </button>
                      <button type="button" className="setup-secondary-btn" onClick={() => startEdit(emp)}>
                        修改
                      </button>
                      <button type="button" className="setup-secondary-btn" onClick={() => void handleToggle(emp)}>
                        {emp.enabled ? "禁用" : "启用"}
                      </button>
                      <button type="button" className="setup-secondary-btn" onClick={() => void handleDelete(emp)}>
                        删除
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            </div>
          ))
        )}
      </section>
    </div>
  );
}

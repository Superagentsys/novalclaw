import { useCallback, useEffect, useMemo, useState } from "react";
import { UiIcon } from "../UiIcon";
import { invokeTauri } from "../../utils/tauri";
import { parseModelSelection, readStoredModelSelection } from "../Chat/ModelPicker";
import {
  AUTOMATION_TEMPLATES,
  WEEKDAY_OPTIONS,
  compileSchedule,
  describeSchedule,
  emptyDraft,
  formatDateTime,
  formatDuration,
  jobStatusLabel,
  localTzOffsetMinutes,
  parseSchedule,
  type AutomationJob,
  type AutomationJobInput,
  type AutomationRun,
  type AutomationTemplate,
  type ScheduleDraft,
} from "./templates";
import "./Automation.css";

type PageTab = "jobs" | "runs";

interface EditorState {
  id?: string;
  name: string;
  description: string;
  prompt: string;
  templateId?: string;
  enabled: boolean;
  schedule: ScheduleDraft;
}

function currentModelRoute(): { provider?: string; model?: string } {
  const parsed = parseModelSelection(readStoredModelSelection());
  return { provider: parsed.providerId, model: parsed.model };
}

function blankEditor(template?: AutomationTemplate): EditorState {
  return {
    name: template?.title ?? "",
    description: template?.description ?? "",
    prompt: template?.prompt ?? "",
    templateId: template?.id,
    enabled: true,
    schedule: template ? { ...template.schedule } : emptyDraft(),
  };
}

function editorFromJob(job: AutomationJob): EditorState {
  return {
    id: job.id,
    name: job.name,
    description: job.description,
    prompt: job.prompt || job.command,
    templateId: job.template_id ?? undefined,
    enabled: job.enabled,
    schedule: parseSchedule(job.schedule),
  };
}

export function Automation() {
  const [tab, setTab] = useState<PageTab>("jobs");
  const [jobs, setJobs] = useState<AutomationJob[]>([]);
  const [runs, setRuns] = useState<AutomationRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [expandedRunId, setExpandedRunId] = useState<string | null>(null);

  const loadAll = useCallback(async () => {
    setLoading(true);
    try {
      const [nextJobs, nextRuns] = await Promise.all([
        invokeTauri<AutomationJob[]>("automation_list_jobs"),
        invokeTauri<AutomationRun[]>("automation_list_runs", { limit: 80 }),
      ]);
      setJobs(nextJobs);
      setRuns(nextRuns);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  const enabledCount = useMemo(
    () => jobs.filter((job) => job.enabled).length,
    [jobs],
  );

  const saveEditor = async () => {
    if (!editor) return;
    const schedule = compileSchedule(editor.schedule);
    const input: AutomationJobInput = {
      id: editor.id,
      name: editor.name,
      schedule,
      prompt: editor.prompt,
      description: editor.description,
      templateId: editor.templateId,
      tzOffsetMinutes: localTzOffsetMinutes(),
      enabled: editor.enabled,
      ...currentModelRoute(),
    };
    setBusyId("save");
    try {
      await invokeTauri<AutomationJob>("automation_upsert_job", { input });
      setEditor(null);
      await loadAll();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  };

  const toggleJob = async (job: AutomationJob) => {
    setBusyId(job.id);
    try {
      await invokeTauri("automation_set_enabled", {
        id: job.id,
        enabled: !job.enabled,
        ...currentModelRoute(),
      });
      await loadAll();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  };

  const runJob = async (job: AutomationJob) => {
    setBusyId(`${job.id}:run`);
    try {
      await invokeTauri("automation_run_now", { id: job.id, ...currentModelRoute() });
      setTab("runs");
      await loadAll();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  };

  const deleteJob = async (job: AutomationJob) => {
    if (!window.confirm(`删除自动化「${job.name}」？相关运行记录也会移除。`)) {
      return;
    }
    setBusyId(`${job.id}:delete`);
    try {
      await invokeTauri("automation_delete_job", { id: job.id });
      await loadAll();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  };

  const clearRuns = async () => {
    if (!window.confirm("清空全部运行记录？")) return;
    setBusyId("clear-runs");
    try {
      await invokeTauri("automation_clear_runs");
      await loadAll();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="automation-page">
      <header className="automation-hero">
        <div>
          <p className="automation-kicker">Automation</p>
          <h1>自动化</h1>
          <p className="automation-subtitle">
            用定时任务把重复工作交给智能体。网关在后台按计划执行，切到别的页面也会继续跑。
          </p>
        </div>
        <div className="automation-hero-stats">
          <span>{jobs.length} 个任务</span>
          <span>{enabledCount} 个启用中</span>
          <span>{runs.length} 条记录</span>
        </div>
      </header>

      <div className="automation-tabs">
        <button
          type="button"
          className={tab === "jobs" ? "is-active" : ""}
          onClick={() => setTab("jobs")}
        >
          定时任务
        </button>
        <button
          type="button"
          className={tab === "runs" ? "is-active" : ""}
          onClick={() => setTab("runs")}
        >
          运行记录
        </button>
      </div>

      {error ? <div className="automation-error">{error}</div> : null}

      {tab === "jobs" ? (
        <>
          <section className="automation-panel">
            {jobs.length === 0 && !loading ? (
              <div className="automation-empty">
                <UiIcon name="clock" size={36} />
                <h2>开始你的第一个自动化任务</h2>
                <p>从下方模板一键创建，或自定义一条定时指令。</p>
                <button type="button" className="automation-primary" onClick={() => setEditor(blankEditor())}>
                  <UiIcon name="plus" size={16} />
                  添加自动化
                </button>
              </div>
            ) : (
              <>
                <div className="automation-panel-head">
                  <h2>我的任务</h2>
                  <button type="button" className="automation-primary" onClick={() => setEditor(blankEditor())}>
                    <UiIcon name="plus" size={16} />
                    添加自动化
                  </button>
                </div>
                {loading ? <p className="automation-muted">正在加载…</p> : null}
                <div className="automation-job-list">
                  {jobs.map((job) => (
                    <article key={job.id} className={`automation-job ${job.enabled ? "" : "is-disabled"}`}>
                      <div className="automation-job-main">
                        <strong>{job.name}</strong>
                        <p>{job.description || job.prompt}</p>
                        <div className="automation-job-meta">
                          <span>{describeSchedule(job.schedule)}</span>
                          <span>下次 {formatDateTime(job.next_run)}</span>
                          <span className={`automation-status automation-status--${job.last_status ?? "idle"}`}>
                            {jobStatusLabel(job.last_status)}
                          </span>
                        </div>
                      </div>
                      <div className="automation-job-actions">
                        <button type="button" onClick={() => void toggleJob(job)} disabled={busyId === job.id}>
                          {job.enabled ? "暂停" : "启用"}
                        </button>
                        <button
                          type="button"
                          onClick={() => void runJob(job)}
                          disabled={busyId === `${job.id}:run`}
                        >
                          立即运行
                        </button>
                        <button type="button" onClick={() => setEditor(editorFromJob(job))}>
                          编辑
                        </button>
                        <button
                          type="button"
                          className="is-danger"
                          onClick={() => void deleteJob(job)}
                          disabled={busyId === `${job.id}:delete`}
                        >
                          删除
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              </>
            )}
          </section>

          <section className="automation-templates">
            <div className="automation-panel-head">
              <h2>自动化任务模板</h2>
              <span className="automation-muted">点选后可再改时间和指令</span>
            </div>
            <div className="automation-template-grid">
              {AUTOMATION_TEMPLATES.map((template) => (
                <button
                  key={template.id}
                  type="button"
                  className="automation-template-card"
                  onClick={() => setEditor(blankEditor(template))}
                >
                  <span className="automation-template-icon">
                    <UiIcon name={template.icon} size={20} />
                  </span>
                  <strong>{template.title}</strong>
                  <p>{template.description}</p>
                  <small>{describeSchedule(compileSchedule(template.schedule))}</small>
                </button>
              ))}
            </div>
          </section>
        </>
      ) : (
        <section className="automation-panel">
          <div className="automation-panel-head">
            <h2>运行记录</h2>
            <button type="button" onClick={() => void clearRuns()} disabled={!runs.length}>
              清空记录
            </button>
          </div>
          {runs.length === 0 ? (
            <div className="automation-empty automation-empty--compact">
              <UiIcon name="history" size={28} />
              <p>还没有运行记录。启用任务或点「立即运行」后会显示在这里。</p>
            </div>
          ) : (
            <div className="automation-run-list">
              {runs.map((run) => {
                const open = expandedRunId === run.id;
                return (
                  <article key={run.id} className="automation-run">
                    <button
                      type="button"
                      className="automation-run-toggle"
                      onClick={() => setExpandedRunId(open ? null : run.id)}
                    >
                      <span className={`automation-status automation-status--${run.status}`}>
                        {jobStatusLabel(run.status)}
                      </span>
                      <strong>{run.job_name}</strong>
                      <span>{formatDateTime(run.started_at)}</span>
                      <span>{formatDuration(run.duration_ms)}</span>
                      <span>{run.trigger === "manual" ? "手动" : "定时"}</span>
                    </button>
                    {open ? (
                      <div className="automation-run-body">
                        {run.error ? <pre className="is-error">{run.error}</pre> : null}
                        {run.output ? <pre>{run.output}</pre> : null}
                        {!run.error && !run.output ? <p className="automation-muted">没有可展示的输出。</p> : null}
                      </div>
                    ) : null}
                  </article>
                );
              })}
            </div>
          )}
        </section>
      )}

      {editor ? (
        <EditorDialog
          editor={editor}
          busy={busyId === "save"}
          onChange={setEditor}
          onClose={() => setEditor(null)}
          onSave={() => void saveEditor()}
        />
      ) : null}
    </div>
  );
}

function EditorDialog({
  editor,
  busy,
  onChange,
  onClose,
  onSave,
}: {
  editor: EditorState;
  busy: boolean;
  onChange: (next: EditorState) => void;
  onClose: () => void;
  onSave: () => void;
}) {
  const patchSchedule = (patch: Partial<ScheduleDraft>) =>
    onChange({ ...editor, schedule: { ...editor.schedule, ...patch } });

  return (
    <div className="automation-modal" role="dialog" aria-modal="true" aria-labelledby="automation-editor-title">
      <div className="automation-modal-card">
        <header>
          <h2 id="automation-editor-title">{editor.id ? "编辑自动化" : "添加自动化"}</h2>
          <button type="button" className="automation-icon-btn" onClick={onClose} aria-label="关闭">
            <UiIcon name="close" size={16} />
          </button>
        </header>
        <label>
          名称
          <input
            value={editor.name}
            onChange={(event) => onChange({ ...editor, name: event.target.value })}
            placeholder="例如：每日 AI 资讯"
          />
        </label>
        <label>
          说明
          <input
            value={editor.description}
            onChange={(event) => onChange({ ...editor, description: event.target.value })}
            placeholder="这条任务会做什么"
          />
        </label>
        <fieldset className="automation-schedule">
          <legend>触发时间</legend>
          <div className="automation-kind-row">
            {(
              [
                ["daily", "每天"],
                ["weekdays", "工作日"],
                ["weekly", "每周"],
                ["interval", "间隔"],
                ["cron", "Cron"],
              ] as Array<[ScheduleDraft["kind"], string]>
            ).map(([kind, label]) => (
              <button
                key={kind}
                type="button"
                className={editor.schedule.kind === kind ? "is-active" : ""}
                onClick={() => patchSchedule({ kind })}
              >
                {label}
              </button>
            ))}
          </div>
          {editor.schedule.kind === "interval" ? (
            <div className="automation-inline">
              <span>每</span>
              <input
                type="number"
                min={1}
                value={editor.schedule.intervalValue}
                onChange={(event) => patchSchedule({ intervalValue: event.target.value })}
              />
              <select
                value={editor.schedule.intervalUnit}
                onChange={(event) =>
                  patchSchedule({ intervalUnit: event.target.value as ScheduleDraft["intervalUnit"] })
                }
              >
                <option value="m">分钟</option>
                <option value="h">小时</option>
                <option value="d">天</option>
              </select>
            </div>
          ) : editor.schedule.kind === "cron" ? (
            <input
              value={editor.schedule.cron}
              onChange={(event) => patchSchedule({ cron: event.target.value })}
              placeholder="0 9 * * *"
            />
          ) : (
            <div className="automation-inline">
              {editor.schedule.kind === "weekly" ? (
                <select
                  value={editor.schedule.weekday}
                  onChange={(event) => patchSchedule({ weekday: event.target.value })}
                >
                  {WEEKDAY_OPTIONS.map((item) => (
                    <option key={item.value} value={item.value}>
                      {item.label}
                    </option>
                  ))}
                </select>
              ) : null}
              <input
                type="number"
                min={0}
                max={23}
                value={editor.schedule.hour}
                onChange={(event) => patchSchedule({ hour: event.target.value })}
              />
              <span>:</span>
              <input
                type="number"
                min={0}
                max={59}
                value={editor.schedule.minute}
                onChange={(event) => patchSchedule({ minute: event.target.value })}
              />
            </div>
          )}
          <p className="automation-muted">将按 {describeSchedule(compileSchedule(editor.schedule))} 触发</p>
        </fieldset>
        <label>
          交给智能体的指令
          <textarea
            rows={7}
            value={editor.prompt}
            onChange={(event) => onChange({ ...editor, prompt: event.target.value })}
            placeholder="描述希望智能体在触发时完成的事情"
          />
        </label>
        <label className="automation-toggle">
          <input
            type="checkbox"
            checked={editor.enabled}
            onChange={(event) => onChange({ ...editor, enabled: event.target.checked })}
          />
          保存后立即启用
        </label>
        <footer>
          <button type="button" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="automation-primary"
            onClick={onSave}
            disabled={busy || !editor.name.trim() || !editor.prompt.trim()}
          >
            {busy ? "保存中…" : "保存"}
          </button>
        </footer>
      </div>
    </div>
  );
}

export default Automation;

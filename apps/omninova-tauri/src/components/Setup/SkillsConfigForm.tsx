import React, { useCallback, useEffect, useMemo, useState } from "react";
import type { SkillsConfig } from "../../types/config";
import { invokeTauri } from "../../utils/tauri";

interface Props {
  config: SkillsConfig;
  onChange: (config: SkillsConfig) => void;
}

interface SkillItem {
  name: string;
  description: string;
  subdomain?: string | null;
}

interface SkillsPackageSummary {
  dir: string;
  total: number;
  names: string[];
  items?: SkillItem[];
}

/** Max cards rendered at once (the library can hold 10k+ skills). */
const RENDER_CAP = 120;

/** Prettify a kebab-case skill id into a readable title. */
function prettyName(name: string): string {
  return name.replace(/[-_]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Pick an emoji for a skill based on its subdomain / keywords. */
function iconFor(item: SkillItem): string {
  const hay = `${item.subdomain ?? ""} ${item.name}`.toLowerCase();
  const table: [RegExp, string][] = [
    [/forensic|incident|dfir|memory|disk/, "🔬"],
    [/malware|reverse|rootkit|ransom/, "🦠"],
    [/network|packet|dns|traffic|firewall/, "🌐"],
    [/web|api|http|sql|xss|ssrf/, "🕸️"],
    [/cloud|aws|azure|gcp|kubernetes|container/, "☁️"],
    [/threat|hunt|intel|siem|detection|log/, "🛰️"],
    [/identity|iam|auth|kerberos|credential|password/, "🔑"],
    [/recon|osint|enumerat|scan/, "🔎"],
    [/exploit|payload|pentest|red.?team|privilege/, "💥"],
    [/phish|email|social/, "🎣"],
    [/crypto|tls|cert|encrypt/, "🔐"],
    [/mobile|android|ios/, "📱"],
    [/report|compliance|govern|stig|audit/, "📋"],
    [/image|vision|photo/, "🖼️"],
    [/paper|research|citation|academic|nature/, "📄"],
  ];
  for (const [re, emoji] of table) {
    if (re.test(hay)) return emoji;
  }
  return "🧩";
}

export const SkillsConfigForm: React.FC<Props> = ({ config, onChange }) => {
  const [importPath, setImportPath] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [summary, setSummary] = useState<SkillsPackageSummary | null>(null);
  const [isLoadingSummary, setIsLoadingSummary] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const refreshSummary = useCallback(async () => {
    if (!config.open_skills_enabled) {
      setSummary(null);
      setSummaryError(null);
      return;
    }
    setIsLoadingSummary(true);
    setSummaryError(null);
    try {
      const payload = await invokeTauri<SkillsPackageSummary>("skills_package_summary");
      setSummary(payload);
    } catch (e) {
      setSummaryError(String(e));
    } finally {
      setIsLoadingSummary(false);
    }
  }, [config.open_skills_enabled]);

  useEffect(() => {
    void refreshSummary();
  }, [refreshSummary, config.open_skills_dir]);

  const allItems: SkillItem[] = useMemo(() => {
    if (summary?.items?.length) return summary.items;
    // Fallback for older backends that only return names.
    return (summary?.names ?? []).map((name) => ({ name, description: "" }));
  }, [summary]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allItems;
    return allItems.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        (s.subdomain ?? "").toLowerCase().includes(q)
    );
  }, [allItems, query]);

  const visible = filtered.slice(0, RENDER_CAP);

  const handleImport = async () => {
    if (!importPath) return;
    setIsImporting(true);
    setImportStatus(null);
    try {
      const result = await invokeTauri<string>("import_skills", { sourceDir: importPath });
      setImportStatus(`✓ ${result}`);
      await refreshSummary();
    } catch (e) {
      setImportStatus(`✗ 导入失败: ${String(e)}`);
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <div className="setup-stack">
      {/* Enable toggle */}
      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>技能扩展</h2>
            <div className="section-subtitle">
              允许 Agent 加载并使用外部技能（SKILL.md 格式）
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={config.open_skills_enabled}
            className={`skill-switch ${config.open_skills_enabled ? "is-on" : ""}`}
            onClick={() =>
              onChange({ ...config, open_skills_enabled: !config.open_skills_enabled })
            }
          >
            <span className="skill-switch-knob" />
          </button>
        </div>
      </section>

      {config.open_skills_enabled && (
        <>
          {/* Basic config */}
          <section className="setup-section">
            <h3>基础配置</h3>
            <div className="setup-grid">
              <label>
                Skills 目录路径
                <input
                  type="text"
                  value={config.open_skills_dir || ""}
                  onChange={(e) => onChange({ ...config, open_skills_dir: e.target.value })}
                  placeholder="~/.omninova/skills"
                />
              </label>
              <label>
                提示词注入模式
                <select
                  value={config.prompt_injection_mode || "full"}
                  onChange={(e) =>
                    onChange({ ...config, prompt_injection_mode: e.target.value })
                  }
                >
                  <option value="full">全量注入 (推荐)</option>
                  <option value="summary">仅注入摘要</option>
                  <option value="disabled">不注入</option>
                </select>
              </label>
            </div>
            <div className="skill-dir-hint">
              扫描目录：<code>{summary?.dir || config.open_skills_dir || "~/.omninova/skills"}</code>
            </div>
          </section>

          {/* Skill gallery */}
          <section className="setup-section">
            <div className="skill-gallery-head">
              <div>
                <h3>技能包</h3>
                <div className="section-subtitle">
                  {isLoadingSummary ? "读取中…" : `共 ${summary?.total ?? 0} 个技能`}
                </div>
              </div>
              <div className="skill-gallery-actions">
                <input
                  type="search"
                  className="skill-search"
                  placeholder="搜索技能名称 / 描述 / 分类…"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  disabled={isLoadingSummary || !!summaryError}
                />
                <button
                  type="button"
                  className="setup-btn setup-btn--secondary"
                  onClick={() => void refreshSummary()}
                  disabled={isLoadingSummary}
                >
                  {isLoadingSummary ? "刷新中…" : "刷新"}
                </button>
              </div>
            </div>

            {summaryError ? (
              <div className="skill-empty skill-empty--error">读取失败：{summaryError}</div>
            ) : isLoadingSummary ? (
              <div className="skill-empty">正在扫描技能目录…</div>
            ) : allItems.length === 0 ? (
              <div className="skill-empty">
                当前未发现可用技能包（包含 SKILL.md 的文件夹）。可在下方从 OpenClaw 导入。
              </div>
            ) : (
              <>
                <div className="skill-grid">
                  {visible.map((s) => (
                    <div className="skill-card" key={s.name} title={s.name}>
                      <div className="skill-card-icon" aria-hidden>
                        {iconFor(s)}
                      </div>
                      <div className="skill-card-body">
                        <div className="skill-card-name">{prettyName(s.name)}</div>
                        <div className="skill-card-desc">
                          {s.description || "（该技能未提供描述）"}
                        </div>
                        {s.subdomain ? (
                          <span className="skill-card-tag">{s.subdomain}</span>
                        ) : null}
                      </div>
                      <span className="skill-card-check" title="已启用">
                        ✓
                      </span>
                    </div>
                  ))}
                </div>
                {filtered.length > visible.length ? (
                  <div className="skill-more">
                    显示前 {visible.length} / 共 {filtered.length} 个匹配，输入关键词以缩小范围
                  </div>
                ) : query ? (
                  <div className="skill-more">{filtered.length} 个匹配</div>
                ) : null}
              </>
            )}
          </section>

          {/* Import */}
          <section className="setup-section">
            <h3>从 OpenClaw 导入</h3>
            <div className="section-subtitle">
              将 OpenClaw 格式的 skills 目录导入到当前工作区
            </div>
            <div className="skill-import-row">
              <input
                type="text"
                value={importPath}
                onChange={(e) => setImportPath(e.target.value)}
                placeholder="/path/to/openclaw/skills"
              />
              <button
                type="button"
                className="setup-btn setup-btn--primary"
                onClick={handleImport}
                disabled={isImporting || !importPath}
              >
                {isImporting ? "导入中…" : "开始导入"}
              </button>
            </div>
            {importStatus && (
              <div
                className={`skill-import-status ${
                  importStatus.includes("✗") ? "is-error" : "is-ok"
                }`}
              >
                {importStatus}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
};

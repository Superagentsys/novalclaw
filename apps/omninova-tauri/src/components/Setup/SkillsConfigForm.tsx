import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  SkillHubCategory,
  SkillHubInstallResult,
  SkillHubItem,
  SkillsConfig,
} from "../../types/config";
import { invokeTauri } from "../../utils/tauri";
import { UiIcon } from "../UiIcon";

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
  slugs?: string[];
}

interface StatusNote {
  tone: "ok" | "error";
  message: string;
}

interface SkillInstallLogEntry {
  id: string;
  at: number;
  skill: string;
  slug: string;
  action: "install" | "update" | "rollback" | "remove";
  status: "running" | "success" | "error";
  detail: string;
}

const SKILL_INSTALL_LOG_KEY = "omninova.skillhub.installLog.v1";

function loadInstallLog(): SkillInstallLogEntry[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(SKILL_INSTALL_LOG_KEY) || "[]");
    return Array.isArray(parsed) ? parsed.slice(0, 60) : [];
  } catch {
    return [];
  }
}

/** Max cards rendered at once (the library can hold 10k+ skills). */
const RENDER_CAP = 120;

/**
 * 预置精选技能包（参照 WorkBuddy 的技能市场设计）。
 * 使用真实存在的 SkillHub slug，网络不可用时也能展示，点击即可从 SkillHub 一键安装。
 */
const PRESET_SKILLS: SkillHubItem[] = [
  {
    name: "建筑设计师·AI 工作台",
    slug: "architect-designer",
    namespace: "user_151a0896",
    description: "把复杂规划条件转化为设计决策，覆盖居住/商业/办公等方案推演。",
    downloads: 469,
    category: "professional",
  },
  {
    name: "自动剪辑视频专家",
    slug: "auto-video-editing-expert",
    namespace: "user_d0d4e3d9",
    description: "平台一键成片、AI 长转短高光、批量混剪的自动剪辑工作流。",
    downloads: 211,
    category: "design-media",
  },
  {
    name: "分镜头脚本生成器",
    slug: "drama-script-generator",
    namespace: "user_15f6f029",
    description: "短剧/短视频剧本全流程创作：大纲、正文细化到分镜头编排。",
    downloads: 160,
    category: "content-creation",
  },
  {
    name: "让 AI 像人一样写作",
    slug: "human-voice-fusion-skill",
    namespace: "user_5cc5abf3",
    description: "四阶段管线消除 AI 味，让文字更自然、更像真人表达。",
    downloads: 108,
    category: "content-creation",
  },
  {
    name: "企业深度研报",
    slug: "enterprise-research-report",
    namespace: "user_5d78968c",
    description: "证据驱动的企业深度调研与研报生成。",
    downloads: 139,
    category: "business-ops",
  },
  {
    name: "代码梳理自查",
    slug: "code-groom",
    namespace: "user_dbceeed3",
    description: "改动代码后按五原则七规则做内部组织梳理，消冗余、拆臃肿。",
    downloads: 90,
    category: "dev-programming",
  },
  {
    name: "客户透镜",
    slug: "enterprise-customer-deep-research",
    namespace: "user_d572d9e3",
    description: "对企业客户做证据驱动的深度调研，穿透股权与实际控制人。",
    downloads: 116,
    category: "business-ops",
  },
  {
    name: "会议时光机",
    slug: "meeting-minutes-maker-2026s2",
    namespace: "user_01440017",
    description: "把会议录音/录像一键变成结构化中文会议资产。",
    downloads: 81,
    category: "office-efficiency",
  },
];

/** Prettify a kebab-case skill id into a readable title. */
function prettyName(name: string): string {
  return name.replace(/[-_]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Restore the original local skill markers used before the visual refresh. */
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
    [/paper|research|citation|academic|nature/, "📚"],
  ];
  for (const [re, icon] of table) {
    if (re.test(hay)) return icon;
  }
  return "🧩";
}

/** Original category marker, used only when SkillHub does not provide a logo. */
function marketIcon(item: SkillHubItem): string {
  const table: Record<string, string> = {
    "office-efficiency": "🗂️",
    "content-creation": "✍️",
    "dev-programming": "💻",
    "data-analysis": "📊",
    "design-media": "🎬",
    "ai-agent": "🤖",
    "knowledge-management": "📚",
    "business-ops": "📈",
    education: "🎓",
    professional: "🏛️",
    "life-service": "🛎️",
    "it-ops-security": "🛡️",
    "pay-skill": "💳",
  };
  return table[item.category ?? ""] ?? "🧩";
}

function safeSkillIconUrl(value?: string | null): string | null {
  if (!value) return null;
  try {
    const parsed = new URL(value);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? parsed.href : null;
  } catch {
    return null;
  }
}

function MarketBrandIcon({ item }: { item: SkillHubItem }) {
  const [imageFailed, setImageFailed] = useState(false);
  const iconUrl = safeSkillIconUrl(item.iconUrl);
  if (iconUrl && !imageFailed) {
    return (
      <img
        src={iconUrl}
        alt=""
        loading="lazy"
        referrerPolicy="no-referrer"
        onError={() => setImageFailed(true)}
      />
    );
  }
  return <span className="skill-original-marker">{marketIcon(item)}</span>;
}

function formatDownloads(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)}w`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

export const SkillsConfigForm: React.FC<Props> = ({ config, onChange }) => {
  const [importPath, setImportPath] = useState("");
  const [importStatus, setImportStatus] = useState<StatusNote | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [summary, setSummary] = useState<SkillsPackageSummary | null>(null);
  const [isLoadingSummary, setIsLoadingSummary] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  // ---- SkillHub marketplace state ----
  const [featured, setFeatured] = useState<SkillHubItem[]>(PRESET_SKILLS);
  const [categories, setCategories] = useState<SkillHubCategory[]>([]);
  const [activeCategory, setActiveCategory] = useState<string>("");
  const [marketKeyword, setMarketKeyword] = useState("");
  const [marketItems, setMarketItems] = useState<SkillHubItem[]>([]);
  const [marketPage, setMarketPage] = useState(1);
  const [marketHasMore, setMarketHasMore] = useState(false);
  const [marketLoading, setMarketLoading] = useState(false);
  const [marketError, setMarketError] = useState<string | null>(null);
  const [installingSlug, setInstallingSlug] = useState<string | null>(null);
  const [rollingBackSlug, setRollingBackSlug] = useState<string | null>(null);
  const [removingSlug, setRemovingSlug] = useState<string | null>(null);
  const [installNote, setInstallNote] = useState<StatusNote | null>(null);
  const [expandedSlugs, setExpandedSlugs] = useState<Set<string>>(() => new Set());
  const [installLog, setInstallLog] = useState<SkillInstallLogEntry[]>(loadInstallLog);
  const [installLogOpen, setInstallLogOpen] = useState(false);

  const MARKET_PAGE_SIZE = 24;

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

  const installedSlugs = useMemo(
    () => new Set(summary?.slugs ?? []),
    [summary]
  );

  useEffect(() => {
    try {
      localStorage.setItem(SKILL_INSTALL_LOG_KEY, JSON.stringify(installLog.slice(0, 60)));
    } catch {
      // Keep the in-memory audit trail when WebView storage is unavailable.
    }
  }, [installLog]);

  const startInstallLog = useCallback(
    (item: SkillHubItem, action: SkillInstallLogEntry["action"]) => {
      const id = `${Date.now()}-${item.slug}-${action}`;
      setInstallLogOpen(true);
      setInstallLog((prev) => [
        {
          id,
          at: Date.now(),
          skill: item.name,
          slug: item.slug,
          action,
          status: "running" as const,
          detail:
            action === "rollback"
              ? "正在恢复上一版本"
              : action === "remove"
                ? "正在移除当前版本与回滚备份"
                : "正在下载并验证技能包",
        },
        ...prev,
      ].slice(0, 60));
      return id;
    },
    []
  );

  const finishInstallLog = useCallback(
    (id: string, status: "success" | "error", detail: string) => {
      setInstallLog((prev) => prev.map((entry) =>
        entry.id === id ? { ...entry, status, detail: detail.slice(0, 500) } : entry
      ));
    },
    []
  );

  // Load categories + featured once the module is enabled.
  useEffect(() => {
    if (!config.open_skills_enabled) return;
    void (async () => {
      try {
        const cats = await invokeTauri<SkillHubCategory[]>("skillhub_category_list");
        if (cats.length) setCategories(cats);
      } catch {
        // categories are optional; ignore failures
      }
      try {
        const feat = await invokeTauri<SkillHubItem[]>("skillhub_browse", {
          source: "featured",
          pageSize: 12,
        });
        if (feat.length) setFeatured(feat);
      } catch {
        // keep preset fallback on failure
      }
    })();
  }, [config.open_skills_enabled]);

  const loadMarket = useCallback(
    async (page: number, category: string, keyword: string, append: boolean) => {
      setMarketLoading(true);
      setMarketError(null);
      try {
        const items = await invokeTauri<SkillHubItem[]>("skillhub_browse", {
          source: "all",
          category: category || undefined,
          keyword: keyword.trim() || undefined,
          page,
          pageSize: MARKET_PAGE_SIZE,
        });
        setMarketHasMore(items.length >= MARKET_PAGE_SIZE);
        setMarketItems((prev) => (append ? [...prev, ...items] : items));
      } catch (e) {
        setMarketError(String(e));
        if (!append) setMarketItems([]);
      } finally {
        setMarketLoading(false);
      }
    },
    []
  );

  // Debounced reload when the module is enabled and filters change.
  const debounceRef = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!config.open_skills_enabled) return;
    window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      setMarketPage(1);
      void loadMarket(1, activeCategory, marketKeyword, false);
    }, 300);
    return () => window.clearTimeout(debounceRef.current);
  }, [config.open_skills_enabled, activeCategory, marketKeyword, loadMarket]);

  const handleLoadMore = () => {
    const next = marketPage + 1;
    setMarketPage(next);
    void loadMarket(next, activeCategory, marketKeyword, true);
  };

  const handleInstall = useCallback(
    async (item: SkillHubItem, isUpdate = false) => {
      const action = isUpdate ? "update" : "install";
      const logId = startInstallLog(item, action);
      setInstallingSlug(item.slug);
      setInstallNote(null);
      try {
        const result = await invokeTauri<SkillHubInstallResult>("skillhub_install_skill", {
          slug: item.slug,
          namespace: item.namespace ?? undefined,
          version: item.version ?? undefined,
        });
        setInstallNote({
          tone: "ok",
          message: `${isUpdate ? "已更新" : "已安装"}「${item.name}」（${result.installed} 个技能）`,
        });
        finishInstallLog(
          logId,
          "success",
          `${isUpdate ? "更新" : "安装"}完成，已验证 ${result.installed} 个 SKILL.md；上一版本会在更新时保留用于回滚。`
        );
        await refreshSummary();
      } catch (e) {
        const detail = String(e);
        setInstallNote({
          tone: "error",
          message: `${isUpdate ? "更新" : "安装"}「${item.name}」失败：${detail}`,
        });
        finishInstallLog(logId, "error", detail);
      } finally {
        setInstallingSlug(null);
      }
    },
    [finishInstallLog, refreshSummary, startInstallLog]
  );

  const handleRollback = useCallback(
    async (item: SkillHubItem) => {
      const logId = startInstallLog(item, "rollback");
      setRollingBackSlug(item.slug);
      setInstallNote(null);
      try {
        const result = await invokeTauri<SkillHubInstallResult>("skillhub_rollback_skill", {
          slug: item.slug,
        });
        setInstallNote({
          tone: "ok",
          message: `已回滚「${item.name}」（${result.installed} 个技能）`,
        });
        finishInstallLog(logId, "success", `已恢复上一版本，并保留被替换版本以便再次切换。`);
        await refreshSummary();
      } catch (e) {
        const detail = String(e);
        setInstallNote({ tone: "error", message: `回滚「${item.name}」失败：${detail}` });
        finishInstallLog(logId, "error", detail);
      } finally {
        setRollingBackSlug(null);
      }
    },
    [finishInstallLog, refreshSummary, startInstallLog]
  );

  const handleRemove = useCallback(
    async (item: SkillHubItem) => {
      if (!window.confirm(`移除「${item.name}」？当前版本和本机保留的回滚版本都会删除。`)) {
        return;
      }
      const logId = startInstallLog(item, "remove");
      setRemovingSlug(item.slug);
      setInstallNote(null);
      try {
        await invokeTauri<SkillHubInstallResult>("skillhub_remove_skill", {
          slug: item.slug,
        });
        setInstallNote({ tone: "ok", message: `已移除「${item.name}」` });
        finishInstallLog(logId, "success", "已删除当前版本与本机回滚备份。");
        await refreshSummary();
      } catch (e) {
        const detail = String(e);
        setInstallNote({ tone: "error", message: `移除「${item.name}」失败：${detail}` });
        finishInstallLog(logId, "error", detail);
      } finally {
        setRemovingSlug(null);
      }
    },
    [finishInstallLog, refreshSummary, startInstallLog]
  );

  const allItems: SkillItem[] = useMemo(() => {
    if (summary?.items?.length) return summary.items;
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
      setImportStatus({ tone: "ok", message: result });
      await refreshSummary();
    } catch (e) {
      setImportStatus({ tone: "error", message: `导入失败: ${String(e)}` });
    } finally {
      setIsImporting(false);
    }
  };

  const renderMarketCard = (item: SkillHubItem) => {
    const installed = installedSlugs.has(item.slug);
    const busy =
      installingSlug === item.slug ||
      rollingBackSlug === item.slug ||
      removingSlug === item.slug;
    const expanded = expandedSlugs.has(item.slug);
    return (
      <div className={`market-card${expanded ? " is-expanded" : ""}`} key={`${item.namespace ?? ""}/${item.slug}`}>
        <div className="market-card-icon" aria-hidden>
          <MarketBrandIcon item={item} />
        </div>
        <div className="market-card-body">
          <div className="market-card-name">{item.name}</div>
          <div className="market-card-desc">
            {item.description || "（该技能未提供描述）"}
          </div>
          <div className="market-card-meta">
            <span className="market-card-downloads">
              下载 {formatDownloads(item.downloads)}
            </span>
            <span className="market-card-tag">版本 {item.version || "最新"}</span>
            {item.category ? (
              <span className="market-card-tag">
                {categories.find((c) => c.key === item.category)?.name ?? item.category}
              </span>
            ) : null}
          </div>
          {expanded ? (
            <dl className="market-card-details">
              <div><dt>来源</dt><dd>SkillHub{item.namespace ? ` / @${item.namespace}` : ""}</dd></div>
              <div><dt>版本</dt><dd>{item.version || "远端接口未提供版本号，安装当前最新版"}</dd></div>
              <div><dt>权限</dt><dd>安装器只写入技能目录并验证 SKILL.md；执行权限由 Agent 自主性策略控制</dd></div>
              <div><dt>更新</dt><dd>更新前保留一个上一版本，可通过回滚恢复</dd></div>
            </dl>
          ) : null}
        </div>
        <div className="market-card-actions">
          <button
            type="button"
            className="market-card-detail-btn"
            aria-expanded={expanded}
            onClick={() => setExpandedSlugs((prev) => {
              const next = new Set(prev);
              if (next.has(item.slug)) next.delete(item.slug);
              else next.add(item.slug);
              return next;
            })}
          >
            {expanded ? "收起" : "详情"}
          </button>
          <button
            type="button"
            className={`market-card-btn${installed ? " is-installed" : ""}`}
            disabled={busy}
            onClick={() => void handleInstall(item, installed)}
            title={installed ? "下载最新版并保留当前版本用于回滚" : "安装到本地技能库"}
          >
            {installingSlug === item.slug ? (
              "处理中…"
            ) : installed ? (
              <><UiIcon name="sync" size={13} /> 更新</>
            ) : (
              <><UiIcon name="plus" size={13} /> 安装</>
            )}
          </button>
          {installed ? (
            <>
              <button
                type="button"
                className="market-card-rollback-btn"
                disabled={busy}
                onClick={() => void handleRollback(item)}
                title="恢复更新前保留的上一版本"
              >
                <UiIcon name="history" size={12} />
                {rollingBackSlug === item.slug ? "回滚中…" : "回滚"}
              </button>
              <button
                type="button"
                className="market-card-remove-btn"
                disabled={busy}
                onClick={() => void handleRemove(item)}
                title="移除当前版本和回滚备份"
              >
                <UiIcon name="delete" size={12} />
                {removingSlug === item.slug ? "移除中…" : "移除"}
              </button>
            </>
          ) : null}
        </div>
      </div>
    );
  };

  return (
    <div className="setup-stack">
      {/* Enable toggle */}
      <section className="setup-section">
        <div className="section-heading">
          <div>
            <h2>技能运行状态</h2>
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

          {/* SkillHub marketplace */}
          <section className="setup-section">
            <div className="skill-gallery-head">
              <div>
                <h3>技能市场 · SkillHub</h3>
                <div className="section-subtitle">
                  浏览并一键安装来自{" "}
                  <a href="https://skillhub.cn/" target="_blank" rel="noreferrer">
                    skillhub.cn
                  </a>{" "}
                  的技能包，安装后自动进入本地技能库。
                </div>
              </div>
            </div>

            {installNote ? (
              <div
                className={`skill-import-status ${installNote.tone === "error" ? "is-error" : "is-ok"}`}
              >
                <UiIcon
                  name={installNote.tone === "error" ? "warning" : "check"}
                  size={14}
                />{" "}
                {installNote.message}
              </div>
            ) : null}

            {/* Featured / preset packs */}
            <div className="market-subhead">精选推荐</div>
            <div className="market-grid market-grid--featured">
              {featured.map(renderMarketCard)}
            </div>

            {/* Browse: search + categories */}
            <div className="market-subhead market-subhead--browse">全部技能</div>
            <div className="market-toolbar">
              <input
                type="search"
                className="skill-search"
                placeholder="搜索技能名称 / 描述…"
                value={marketKeyword}
                onChange={(e) => setMarketKeyword(e.target.value)}
              />
            </div>
            <div className="market-chips">
              <button
                type="button"
                className={`market-chip${activeCategory === "" ? " is-active" : ""}`}
                onClick={() => setActiveCategory("")}
              >
                全部
              </button>
              {categories.map((c) => (
                <button
                  key={c.key}
                  type="button"
                  className={`market-chip${activeCategory === c.key ? " is-active" : ""}`}
                  onClick={() => setActiveCategory(c.key)}
                >
                  {c.name}
                </button>
              ))}
            </div>

            {marketError ? (
              <div className="skill-empty skill-empty--error">
                加载 SkillHub 失败：{marketError}
              </div>
            ) : marketItems.length === 0 && marketLoading ? (
              <div className="skill-empty">正在加载 SkillHub 技能…</div>
            ) : marketItems.length === 0 ? (
              <div className="skill-empty">
                {marketKeyword.trim() || activeCategory
                  ? "没有匹配的技能，换个关键词或分类试试。"
                  : "SkillHub 暂未返回技能，请稍后重新加载。"}
                {!marketKeyword.trim() && !activeCategory ? (
                  <button
                    type="button"
                    className="setup-btn setup-btn--secondary"
                    onClick={() => void loadMarket(1, "", "", false)}
                  >
                    重新加载
                  </button>
                ) : null}
              </div>
            ) : (
              <>
                <div className="market-grid">{marketItems.map(renderMarketCard)}</div>
                {marketHasMore ? (
                  <div className="market-more">
                    <button
                      type="button"
                      className="setup-btn setup-btn--secondary"
                      onClick={handleLoadMore}
                      disabled={marketLoading}
                    >
                      {marketLoading ? "加载中…" : "加载更多"}
                    </button>
                  </div>
                ) : null}
              </>
            )}

            <details
              className="market-install-log"
              open={installLogOpen}
              onToggle={(event) => setInstallLogOpen(event.currentTarget.open)}
            >
              <summary>
                <span><UiIcon name="history" size={14} /> 安装日志</span>
                <small>{installLog.length ? `${installLog.length} 条本机记录` : "暂无记录"}</small>
              </summary>
              <div className="market-install-log-body">
                {installLog.length ? (
                  <>
                    <ol>
                      {installLog.map((entry) => (
                        <li key={entry.id} data-status={entry.status}>
                          <span className="market-install-log-icon">
                            <UiIcon
                              name={entry.status === "success" ? "check" : entry.status === "error" ? "warning" : "sync"}
                              size={12}
                            />
                          </span>
                          <span>
                            <strong>
                              {entry.action === "install"
                                ? "安装"
                                : entry.action === "update"
                                  ? "更新"
                                  : entry.action === "rollback"
                                    ? "回滚"
                                    : "移除"}
                              「{entry.skill}」
                            </strong>
                            <small>{entry.detail}</small>
                          </span>
                          <time>{new Date(entry.at).toLocaleString("zh-CN", { hour12: false })}</time>
                        </li>
                      ))}
                    </ol>
                    <button type="button" onClick={() => setInstallLog([])}>清空安装日志</button>
                  </>
                ) : (
                  <div className="skill-empty">安装、更新和回滚结果会记录在本机，不上传到市场。</div>
                )}
              </div>
            </details>
          </section>

          {/* Installed gallery */}
          <section className="setup-section">
            <div className="skill-gallery-head">
              <div>
                <h3>已安装技能</h3>
                <div className="section-subtitle">
                  {isLoadingSummary ? "读取中…" : `本地共 ${summary?.total ?? 0} 个技能`}
                </div>
              </div>
              <div className="skill-gallery-actions">
                <input
                  type="search"
                  className="skill-search"
                  placeholder="搜索已安装技能…"
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
                本地暂无技能。可从上方 SkillHub 市场安装，或在下方从 OpenClaw 导入。
              </div>
            ) : (
              <>
                <div className="skill-grid">
                  {visible.map((s) => (
                    <div className="skill-card" key={s.name} title={s.name}>
                      <div className="skill-card-icon" aria-hidden>
                        <span className="skill-original-marker">{iconFor(s)}</span>
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
                        <UiIcon name="check" size={11} />
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
                className={`skill-import-status ${importStatus.tone === "error" ? "is-error" : "is-ok"}`}
              >
                <UiIcon
                  name={importStatus.tone === "error" ? "warning" : "check"}
                  size={14}
                />{" "}
                {importStatus.message}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
};

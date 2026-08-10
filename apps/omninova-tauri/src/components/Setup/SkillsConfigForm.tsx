import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  SkillHubCategory,
  SkillHubInstallResult,
  SkillHubItem,
  SkillsConfig,
} from "../../types/config";
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
  slugs?: string[];
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

/** Emoji fallback for a marketplace category. */
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

function formatDownloads(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)}w`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

export const SkillsConfigForm: React.FC<Props> = ({ config, onChange }) => {
  const [importPath, setImportPath] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
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
  const [installNote, setInstallNote] = useState<string | null>(null);

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
    async (item: SkillHubItem) => {
      setInstallingSlug(item.slug);
      setInstallNote(null);
      try {
        const result = await invokeTauri<SkillHubInstallResult>("skillhub_install_skill", {
          slug: item.slug,
          namespace: item.namespace ?? undefined,
        });
        setInstallNote(`✓ 已安装「${item.name}」（${result.installed} 个技能）`);
        await refreshSummary();
      } catch (e) {
        setInstallNote(`✗ 安装「${item.name}」失败：${String(e)}`);
      } finally {
        setInstallingSlug(null);
      }
    },
    [refreshSummary]
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
      setImportStatus(`✓ ${result}`);
      await refreshSummary();
    } catch (e) {
      setImportStatus(`✗ 导入失败: ${String(e)}`);
    } finally {
      setIsImporting(false);
    }
  };

  const renderMarketCard = (item: SkillHubItem) => {
    const installed = installedSlugs.has(item.slug);
    const busy = installingSlug === item.slug;
    return (
      <div className="market-card" key={`${item.namespace ?? ""}/${item.slug}`} title={item.name}>
        <div className="market-card-icon" aria-hidden>
          {item.iconUrl ? (
            <img src={item.iconUrl} alt="" loading="lazy" />
          ) : (
            <span>{marketIcon(item)}</span>
          )}
        </div>
        <div className="market-card-body">
          <div className="market-card-name">{item.name}</div>
          <div className="market-card-desc">
            {item.description || "（该技能未提供描述）"}
          </div>
          <div className="market-card-meta">
            <span className="market-card-downloads">↓ {formatDownloads(item.downloads)}</span>
            {item.category ? (
              <span className="market-card-tag">
                {categories.find((c) => c.key === item.category)?.name ?? item.category}
              </span>
            ) : null}
          </div>
        </div>
        <button
          type="button"
          className={`market-card-btn${installed ? " is-installed" : ""}`}
          disabled={busy || installed}
          onClick={() => void handleInstall(item)}
          title={installed ? "已安装" : "安装到本地技能库"}
        >
          {installed ? "✓ 已安装" : busy ? "安装中…" : "+ 安装"}
        </button>
      </div>
    );
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
                className={`skill-import-status ${
                  installNote.includes("✗") ? "is-error" : "is-ok"
                }`}
              >
                {installNote}
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
              <div className="skill-empty">没有匹配的技能，换个关键词或分类试试。</div>
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

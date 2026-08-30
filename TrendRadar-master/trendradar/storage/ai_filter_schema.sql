-- AI 智能筛选相关表结构
-- 在 news 库中创建，与 news_items 同库

-- ============================================
-- AI 筛选兴趣标签表
-- 存储从用户兴趣描述中 AI 提取的结构化标签
-- 按版本管理，提示词变更时旧版本标记 deprecated
-- 支持多兴趣文件隔离（interests_file 区分不同文件的标签集）
-- ============================================
CREATE TABLE IF NOT EXISTS ai_filter_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tag TEXT NOT NULL,                    -- 标签名，如 "AI/大模型"
    description TEXT DEFAULT '',          -- 标签描述，AI 分类时参考
    priority INTEGER NOT NULL DEFAULT 9999, -- 标签优先级（值越小优先级越高）
    status TEXT DEFAULT 'active',        -- active / deprecated
    deprecated_at TEXT,                   -- 废弃时间
    version INTEGER NOT NULL,            -- 版本号，提示词变更时 +1
    prompt_hash TEXT NOT NULL,           -- 兴趣描述文件的 hash（格式: filename:md5）
    interests_file TEXT NOT NULL DEFAULT 'ai_interests.txt',  -- 关联的兴趣文件名
    created_at TEXT NOT NULL
);

-- ============================================
-- AI 筛选分类结果表
-- 每条新闻 × 每个标签 = 一行
-- 引用 news_items.id 或 rss_items.id（通过 source_type 区分）
-- ============================================
CREATE TABLE IF NOT EXISTS ai_filter_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    news_item_id INTEGER NOT NULL,       -- 引用 news_items.id 或 rss_items.id
    source_type TEXT NOT NULL DEFAULT 'hotlist',  -- hotlist / rss
    tag_id INTEGER NOT NULL,             -- 引用 ai_filter_tags.id
    relevance_score REAL DEFAULT 0,      -- 相关度 0.0 ~ 1.0
    status TEXT DEFAULT 'active',        -- active / deprecated
    deprecated_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(news_item_id, source_type, tag_id)
);

-- ============================================
-- AI 筛选已分析新闻记录表
-- 记录所有已被 AI 分析过的新闻（无论匹配与否）
-- 用于去重，避免重复发送给 AI 浪费 token
-- ============================================
CREATE TABLE IF NOT EXISTS ai_filter_analyzed_news (
    news_item_id INTEGER NOT NULL,       -- 引用 news_items.id 或 rss_items.id
    source_type TEXT NOT NULL DEFAULT 'hotlist',  -- hotlist / rss
    interests_file TEXT NOT NULL DEFAULT 'ai_interests.txt',  -- 关联的兴趣文件
    prompt_hash TEXT NOT NULL,           -- 分析时使用的标签集 hash
    matched INTEGER NOT NULL DEFAULT 0,  -- 是否匹配: 0=不匹配, 1=匹配
    created_at TEXT NOT NULL,
    PRIMARY KEY (news_item_id, source_type, interests_file)
);

-- ============================================
-- 索引
-- ============================================
CREATE INDEX IF NOT EXISTS idx_ai_filter_tags_status ON ai_filter_tags(status);
CREATE INDEX IF NOT EXISTS idx_ai_filter_tags_version ON ai_filter_tags(version);
CREATE INDEX IF NOT EXISTS idx_ai_filter_tags_file ON ai_filter_tags(interests_file, status);
CREATE INDEX IF NOT EXISTS idx_ai_filter_tags_priority ON ai_filter_tags(interests_file, status, priority);
CREATE INDEX IF NOT EXISTS idx_ai_filter_results_status ON ai_filter_results(status);
CREATE INDEX IF NOT EXISTS idx_ai_filter_results_news ON ai_filter_results(news_item_id, source_type);
CREATE INDEX IF NOT EXISTS idx_ai_filter_results_tag ON ai_filter_results(tag_id);
CREATE INDEX IF NOT EXISTS idx_analyzed_news_lookup ON ai_filter_analyzed_news(source_type, interests_file);
CREATE INDEX IF NOT EXISTS idx_analyzed_news_hash ON ai_filter_analyzed_news(interests_file, prompt_hash);

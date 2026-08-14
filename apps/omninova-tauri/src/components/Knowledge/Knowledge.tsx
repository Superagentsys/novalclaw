import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import { UiIcon } from "../UiIcon";
import { invokeTauri, isTauriEnvironment } from "../../utils/tauri";
import "./Knowledge.css";

export interface KnowledgeDocument {
  id: string;
  title: string;
  collection: string;
  source: string;
  source_path?: string | null;
  kind: string;
  tags: string[];
  preview: string;
  char_count: number;
  chunk_count: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface KnowledgeHit {
  document_id: string;
  title: string;
  collection: string;
  chunk_index: number;
  heading?: string | null;
  snippet: string;
  score: number;
}

interface EditorState {
  id?: string;
  title: string;
  collection: string;
  tags: string;
  content: string;
  enabled: boolean;
}

function blankEditor(collection: string): EditorState {
  return {
    title: "",
    collection: collection === "all" ? "default" : collection || "default",
    tags: "",
    content: "",
    enabled: true,
  };
}

function formatStamp(value: string) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function Knowledge() {
  const [docs, setDocs] = useState<KnowledgeDocument[]>([]);
  const [collections, setCollections] = useState<string[]>([]);
  const [collection, setCollection] = useState("all");
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<KnowledgeHit[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ document: KnowledgeDocument; content: string } | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const loadList = useCallback(async () => {
    setLoading(true);
    try {
      const [nextDocs, nextCollections] = await Promise.all([
        invokeTauri<KnowledgeDocument[]>("knowledge_list", {
          collection: collection === "all" ? undefined : collection,
        }),
        invokeTauri<string[]>("knowledge_collections"),
      ]);
      setDocs(nextDocs);
      setCollections(nextCollections);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [collection]);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    void invokeTauri<{ document: KnowledgeDocument; content: string }>("knowledge_get", {
      id: selectedId,
    })
      .then((next) => {
        if (!cancelled) setDetail(next);
      })
      .catch((reason) => {
        if (!cancelled) setError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const runSearch = async () => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    setBusy("search");
    try {
      const next = await invokeTauri<KnowledgeHit[]>("knowledge_search", {
        query: q,
        collection: collection === "all" ? undefined : collection,
        limit: 16,
      });
      setHits(next);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const saveEditor = async () => {
    if (!editor) return;
    if (!editor.title.trim() || !editor.content.trim()) {
      setError("标题和正文不能为空。");
      return;
    }
    setBusy("save");
    try {
      const saved = await invokeTauri<KnowledgeDocument>("knowledge_upsert", {
        input: {
          id: editor.id,
          title: editor.title.trim(),
          collection: editor.collection.trim() || "default",
          tags: editor.tags
            .split(",")
            .map((tag) => tag.trim())
            .filter(Boolean),
          content: editor.content,
          enabled: editor.enabled,
          source: editor.id ? undefined : "note",
          kind: "md",
        },
      });
      setEditor(null);
      setSelectedId(saved.id);
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const importPaths = async (paths: string[]) => {
    if (!paths.length) return;
    setBusy("import");
    try {
      await invokeTauri("knowledge_import", {
        paths,
        collection: collection === "all" ? "default" : collection,
      });
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const handleImportClick = async () => {
    if (isTauriEnvironment()) {
      try {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({
          multiple: true,
          title: "导入到知识库",
          filters: [
            { name: "Documents", extensions: ["md", "txt", "markdown", "json", "csv", "html", "pdf"] },
          ],
        });
        if (selected == null) return;
        const paths = Array.isArray(selected) ? selected : [selected];
        await importPaths(paths);
      } catch (reason) {
        setError(String(reason));
      }
      return;
    }
    fileInputRef.current?.click();
  };

  const handleWebFiles = async (event: ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files;
    event.target.value = "";
    if (!files?.length) return;
    setBusy("import");
    try {
      const payloads = await Promise.all(
        Array.from(files).map(async (file) => ({
          name: file.name,
          content: await file.text(),
        }))
      );
      await invokeTauri("knowledge_import", {
        files: payloads,
        collection: collection === "all" ? "default" : collection,
      });
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const toggleEnabled = async (doc: KnowledgeDocument) => {
    setBusy(doc.id);
    try {
      await invokeTauri("knowledge_set_enabled", { id: doc.id, enabled: !doc.enabled });
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const deleteDoc = async (doc: KnowledgeDocument) => {
    if (!window.confirm(`删除知识库文档「${doc.title}」？`)) return;
    setBusy(`${doc.id}:delete`);
    try {
      await invokeTauri("knowledge_delete", { id: doc.id });
      if (selectedId === doc.id) {
        setSelectedId(null);
        setDetail(null);
      }
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const editSelected = () => {
    if (!detail) return;
    setEditor({
      id: detail.document.id,
      title: detail.document.title,
      collection: detail.document.collection,
      tags: detail.document.tags.join(", "),
      content: detail.content,
      enabled: detail.document.enabled,
    });
  };

  const visibleHits = useMemo(
    () => (query.trim() ? hits : []),
    [hits, query]
  );

  return (
    <div className="knowledge-page">
      <header className="knowledge-hero">
        <div>
          <p className="knowledge-kicker">Local retrieval</p>
          <h1>知识库</h1>
          <p className="knowledge-subtitle">
            把笔记、手册和项目文档入库后，对话里的 Agent 会通过 <code>knowledge_search</code> 引用原文片段，而不是凭空编造。
          </p>
        </div>
        <div className="knowledge-hero-stats">
          <span>{docs.length} 篇文档</span>
          <span>{docs.reduce((sum, doc) => sum + doc.chunk_count, 0)} 个片段</span>
          <span>{collections.length} 个分类</span>
        </div>
      </header>

      {error ? <div className="knowledge-error">{error}</div> : null}

      <div className="knowledge-toolbar">
        <form
          className="knowledge-search"
          onSubmit={(event) => {
            event.preventDefault();
            void runSearch();
          }}
        >
          <UiIcon name="search" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="检索知识库…"
            aria-label="检索知识库"
          />
          <button type="submit" disabled={busy === "search"}>
            检索
          </button>
        </form>
        <button type="button" className="knowledge-primary" onClick={() => setEditor(blankEditor(collection))}>
          <UiIcon name="plus" />
          新建笔记
        </button>
        <button type="button" onClick={() => void handleImportClick()} disabled={busy === "import"}>
          导入文件
        </button>
        <input
          ref={fileInputRef}
          type="file"
          multiple
          hidden
          accept=".md,.txt,.markdown,.json,.csv,.html,text/plain"
          onChange={(event) => void handleWebFiles(event)}
        />
      </div>

      <div className="knowledge-tabs">
        <button
          type="button"
          className={collection === "all" ? "is-active" : ""}
          onClick={() => setCollection("all")}
        >
          全部
        </button>
        {collections.map((name) => (
          <button
            key={name}
            type="button"
            className={collection === name ? "is-active" : ""}
            onClick={() => setCollection(name)}
          >
            {name}
          </button>
        ))}
      </div>

      {visibleHits.length > 0 ? (
        <section className="knowledge-hits" aria-label="检索结果">
          {visibleHits.map((hit) => (
            <button
              key={`${hit.document_id}-${hit.chunk_index}`}
              type="button"
              className="knowledge-hit"
              onClick={() => setSelectedId(hit.document_id)}
            >
              <strong>{hit.title}</strong>
              <span>
                {hit.collection}
                {hit.heading ? ` · ${hit.heading}` : ""}
              </span>
              <p>{hit.snippet}</p>
            </button>
          ))}
        </section>
      ) : null}

      <div className="knowledge-split">
        <section className="knowledge-list" aria-label="文档列表">
          {loading ? (
            <p className="knowledge-muted">正在加载…</p>
          ) : docs.length === 0 ? (
            <div className="knowledge-empty">
              <h2>还没有文档</h2>
              <p>新建一条笔记，或导入 Markdown / TXT / PDF。</p>
            </div>
          ) : (
            docs.map((doc) => (
              <article
                key={doc.id}
                className={`knowledge-card ${selectedId === doc.id ? "is-active" : ""} ${doc.enabled ? "" : "is-disabled"}`}
                onClick={() => setSelectedId(doc.id)}
              >
                <div className="knowledge-card-head">
                  <h2>{doc.title}</h2>
                  <span>{doc.collection}</span>
                </div>
                <p>{doc.preview || "空文档"}</p>
                <div className="knowledge-card-meta">
                  <span>{doc.chunk_count} 片段</span>
                  <span>{doc.char_count} 字</span>
                  <span>{doc.enabled ? "已启用" : "已停用"}</span>
                </div>
              </article>
            ))
          )}
        </section>

        <section className="knowledge-preview" aria-label="文档预览">
          {detail ? (
            <>
              <div className="knowledge-preview-head">
                <div>
                  <p className="knowledge-kicker">{detail.document.collection}</p>
                  <h2>{detail.document.title}</h2>
                  <p className="knowledge-muted">
                    更新于 {formatStamp(detail.document.updated_at)} · {detail.document.source}
                    {detail.document.tags.length ? ` · ${detail.document.tags.join(" / ")}` : ""}
                  </p>
                </div>
                <div className="knowledge-preview-actions">
                  <button type="button" onClick={editSelected}>
                    编辑
                  </button>
                  <button type="button" onClick={() => void toggleEnabled(detail.document)} disabled={busy === detail.document.id}>
                    {detail.document.enabled ? "停用" : "启用"}
                  </button>
                  <button type="button" onClick={() => void deleteDoc(detail.document)}>
                    删除
                  </button>
                </div>
              </div>
              <pre className="knowledge-body">{detail.content}</pre>
            </>
          ) : (
            <div className="knowledge-empty">
              <h2>选择一篇文档</h2>
              <p>检索结果或左侧列表都可以打开原文。</p>
            </div>
          )}
        </section>
      </div>

      {editor ? (
        <div className="knowledge-modal" role="dialog" aria-modal="true" aria-labelledby="knowledge-editor-title">
          <div className="knowledge-modal-card">
            <header>
              <h2 id="knowledge-editor-title">{editor.id ? "编辑笔记" : "新建笔记"}</h2>
            </header>
            <label>
              标题
              <input
                value={editor.title}
                onChange={(event) => setEditor({ ...editor, title: event.target.value })}
              />
            </label>
            <div className="knowledge-editor-row">
              <label>
                分类
                <input
                  value={editor.collection}
                  onChange={(event) => setEditor({ ...editor, collection: event.target.value })}
                />
              </label>
              <label>
                标签（逗号分隔）
                <input
                  value={editor.tags}
                  onChange={(event) => setEditor({ ...editor, tags: event.target.value })}
                />
              </label>
            </div>
            <label>
              正文
              <textarea
                value={editor.content}
                onChange={(event) => setEditor({ ...editor, content: event.target.value })}
                rows={14}
              />
            </label>
            <footer>
              <button type="button" onClick={() => setEditor(null)}>
                取消
              </button>
              <button type="button" className="knowledge-primary" onClick={() => void saveEditor()} disabled={busy === "save"}>
                保存入库
              </button>
            </footer>
          </div>
        </div>
      ) : null}
    </div>
  );
}

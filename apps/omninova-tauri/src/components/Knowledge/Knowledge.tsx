import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
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

interface KnowledgeDocumentDetail {
  document: KnowledgeDocument;
  content: string;
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
  const [detail, setDetail] = useState<KnowledgeDocumentDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailReloadKey, setDetailReloadKey] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [editorBaseline, setEditorBaseline] = useState("");
  const [searchAttempted, setSearchAttempted] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const editorDirty = Boolean(editor && JSON.stringify(editor) !== editorBaseline);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Sorting state
  const [sortBy, setSortBy] = useState<"updated" | "title" | "size">("updated");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("desc");

  // Batch selection state
  const [batchMode, setBatchMode] = useState(false);
  const [selectedDocs, setSelectedDocs] = useState<Set<string>>(new Set());

  // Sort documents
  const sortedDocs = useMemo(() => {
    return [...docs].sort((a, b) => {
      let cmp = 0;
      if (sortBy === "updated") {
        cmp = a.updated_at.localeCompare(b.updated_at);
      } else if (sortBy === "title") {
        cmp = a.title.localeCompare(b.title);
      } else if (sortBy === "size") {
        cmp = a.char_count - b.char_count;
      }
      return sortOrder === "asc" ? cmp : -cmp;
    });
  }, [docs, sortBy, sortOrder]);

  const openEditor = useCallback((next: EditorState) => {
    setEditor(next);
    setEditorBaseline(JSON.stringify(next));
  }, []);

  const closeEditor = useCallback(() => {
    if (editorDirty && !window.confirm("当前笔记有未保存的修改，确定放弃吗？")) return;
    setEditor(null);
    setEditorBaseline("");
  }, [editorDirty]);

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
    setHits([]);
    setSearchAttempted(false);
  }, [collection]);

  useEffect(() => {
    if (!editorDirty) return;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeEditor();
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("beforeunload", handleBeforeUnload);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [closeEditor, editorDirty]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      setDetailError(null);
      setDetailLoading(false);
      return;
    }
    let cancelled = false;
    setDetail(null);
    setDetailError(null);
    setDetailLoading(true);
    void invokeTauri<KnowledgeDocumentDetail>("knowledge_get", {
      id: selectedId,
    })
      .then((next) => {
        if (cancelled) return;
        if (next.document.char_count > 0 && !next.content.trim()) {
          throw new Error("正文读取为空，已停止编辑以避免覆盖原文。请重试或检查知识库文件。");
        }
        setDetail(next);
      })
      .catch((reason) => {
        if (cancelled) return;
        const message = String(reason);
        setDetail(null);
        setDetailError(message);
        setError(message);
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, detailReloadKey]);

  const runSearch = async () => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      setSearchAttempted(false);
      return;
    }
    setBusy("search");
    setSearchAttempted(true);
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
      setEditorBaseline("");
      setDetail({ document: saved, content: editor.content.trim() });
      setDetailError(null);
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
    openEditor({
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

  // Keyboard shortcut: Ctrl/Cmd+K to focus search
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // Highlight search query in text
  const highlightMatch = (text: string, searchQuery: string) => {
    if (!searchQuery.trim()) return text;
    const parts = text.split(new RegExp(`(${searchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi"));
    return parts.map((part, i) =>
      part.toLowerCase() === searchQuery.toLowerCase() ? (
        <mark key={i} style={{ background: "var(--ui-accent-soft)", color: "var(--ui-accent-strong)", padding: "0 2px", borderRadius: "2px" }}>{part}</mark>
      ) : part
    );
  };

  // Batch selection handlers
  const toggleBatchMode = () => {
    setBatchMode(!batchMode);
    if (batchMode) {
      setSelectedDocs(new Set());
    }
  };

  const toggleDocSelection = (docId: string) => {
    setSelectedDocs(prev => {
      const next = new Set(prev);
      if (next.has(docId)) {
        next.delete(docId);
      } else {
        next.add(docId);
      }
      return next;
    });
  };

  const deselectAllDocs = () => {
    setSelectedDocs(new Set());
  };

  const moveDocumentsToCollection = async (
    documents: KnowledgeDocument[],
    targetCollection: string,
  ) => {
    // Read every canonical body before writing metadata. An empty content value
    // would replace the stored body and remove all indexed chunks.
    const details = await Promise.all(
      documents.map((doc) =>
        invokeTauri<KnowledgeDocumentDetail>("knowledge_get", { id: doc.id })
      )
    );
    for (const { document, content } of details) {
      await invokeTauri("knowledge_upsert", {
        input: {
          id: document.id,
          title: document.title,
          collection: targetCollection,
          tags: document.tags,
          content,
          enabled: document.enabled,
          source: document.source,
          sourcePath: document.source_path,
          kind: document.kind,
        },
      });
    }
  };

  const deleteSelectedDocs = async () => {
    if (selectedDocs.size === 0) return;
    if (!window.confirm(`确定要删除选中的 ${selectedDocs.size} 篇文档吗？`)) return;
    setBusy("batch-delete");
    try {
      for (const docId of selectedDocs) {
        await invokeTauri("knowledge_delete", { id: docId });
      }
      setSelectedDocs(new Set());
      setBatchMode(false);
      if (selectedId && selectedDocs.has(selectedId)) {
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

  const moveSelectedDocs = async (targetCollection: string) => {
    if (selectedDocs.size === 0) return;
    setBusy("batch-move");
    try {
      const documents = docs.filter((doc) => selectedDocs.has(doc.id));
      await moveDocumentsToCollection(documents, targetCollection);
      setSelectedDocs(new Set());
      setBatchMode(false);
      setDetailReloadKey((current) => current + 1);
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  // Rename a collection (move all docs from old name to new name)
  const renameCollection = async (oldName: string, newName: string) => {
    setBusy(`rename:${oldName}`);
    try {
      const docsInCollection = await invokeTauri<KnowledgeDocument[]>("knowledge_list", {
        collection: oldName,
      });
      await moveDocumentsToCollection(docsInCollection, newName);
      if (collection === oldName) {
        setCollection(newName);
      }
      setDetailReloadKey((current) => current + 1);
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  // Delete a collection (move all docs to default)
  const deleteCollection = async (name: string) => {
    setBusy(`delete:${name}`);
    try {
      const docsInCollection = await invokeTauri<KnowledgeDocument[]>("knowledge_list", {
        collection: name,
      });
      await moveDocumentsToCollection(docsInCollection, "default");
      if (collection === name) {
        setCollection("all");
      }
      setDetailReloadKey((current) => current + 1);
      await loadList();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

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
            ref={searchInputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSearchAttempted(false);
            }}
            placeholder="检索知识库…"
            aria-label="检索知识库"
          />
          <button type="submit" disabled={busy === "search"}>
            {busy === "search" ? "检索中…" : "检索"}
          </button>
          <span className="knowledge-search-hint">
            <kbd>Ctrl</kbd><kbd>K</kbd>
          </span>
        </form>
        <div className="knowledge-toolbar-actions">
          <button
            type="button"
            className={batchMode ? "is-active" : ""}
            onClick={toggleBatchMode}
            title="批量选择"
          >
            <UiIcon name="check" />
            {batchMode ? "取消选择" : "批量操作"}
          </button>
          <button type="button" className="knowledge-primary" onClick={() => openEditor(blankEditor(collection))}>
            <UiIcon name="plus" />
            新建笔记
          </button>
          <button type="button" onClick={() => void handleImportClick()} disabled={busy === "import"}>
            导入文件
          </button>
        </div>
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
          <div key={name} className="knowledge-tab-wrapper">
            <button
              type="button"
              className={`knowledge-tab ${collection === name ? "is-active" : ""}`}
              onClick={() => setCollection(name)}
            >
              {name}
            </button>
            <div className="knowledge-collection-actions">
              <button
                type="button"
                className="knowledge-collection-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  const newName = window.prompt("重命名分类：", name);
                  if (newName?.trim() && newName !== name) {
                    void renameCollection(name, newName.trim());
                  }
                }}
                title="重命名"
              >
                <UiIcon name="edit" size={10} />
              </button>
              <button
                type="button"
                className="knowledge-collection-btn danger"
                onClick={(e) => {
                  e.stopPropagation();
                  if (window.confirm(`确定要删除分类「${name}」吗？该分类下的文档将移至 default。`)) {
                    void deleteCollection(name);
                  }
                }}
                title="删除"
              >
                <UiIcon name="delete" size={10} />
              </button>
            </div>
          </div>
        ))}
        <button
          type="button"
          className="knowledge-tab-add"
          onClick={() => {
            const name = window.prompt("输入新分类名称：");
            if (name?.trim()) {
              setCollection(name.trim());
            }
          }}
          title="新建分类"
        >
          <UiIcon name="plus" size={14} />
        </button>
      </div>

      {visibleHits.length > 0 ? (
        <section className="knowledge-search-panel" aria-label="检索结果" aria-live="polite">
          <header><strong>检索结果</strong><span>{visibleHits.length} 个匹配片段</span></header>
          <div className="knowledge-hits">
          {visibleHits.map((hit) => (
            <button
              key={`${hit.document_id}-${hit.chunk_index}`}
              type="button"
              className="knowledge-hit"
              onClick={() => setSelectedId(hit.document_id)}
            >
              <strong>{highlightMatch(hit.title, query)}</strong>
              <span>
                {hit.collection}
                {hit.heading ? <> · {highlightMatch(hit.heading, query)}</> : null}
              </span>
              <p>{highlightMatch(hit.snippet, query)}</p>
            </button>
          ))}
          </div>
        </section>
      ) : searchAttempted && query.trim() && busy !== "search" ? (
        <div className="knowledge-search-empty" role="status">
          未找到与“{query.trim()}”匹配的内容，可更换关键词或分类后重试。
        </div>
      ) : null}

      <div className="knowledge-split">
        <section className="knowledge-list" aria-label="文档列表">
          {loading ? (
            <p className="knowledge-muted">正在加载…</p>
          ) : docs.length === 0 ? (
            <div className="knowledge-empty">
              <div className="knowledge-empty-icon">
                <UiIcon name="knowledge" size={36} />
              </div>
              <h2>还没有文档</h2>
              <p>新建一条笔记，或导入 Markdown / TXT / PDF。</p>
              <button type="button" className="knowledge-primary" onClick={() => openEditor(blankEditor(collection))}>
                <UiIcon name="plus" size={14} />
                创建第一个文档
              </button>
            </div>
          ) : (
            <>
              {batchMode && selectedDocs.size > 0 && (
                <div className="knowledge-batch-bar">
                  <div className="knowledge-batch-bar-left">
                    <UiIcon name="check" size={14} />
                    已选择 {selectedDocs.size} 篇文档
                  </div>
                  <div className="knowledge-batch-bar-actions">
                    <button type="button" onClick={() => {
                      const target = window.prompt("移动到分类：");
                      if (target?.trim()) moveSelectedDocs(target.trim());
                    }}>
                      移动到
                    </button>
                    <button type="button" onClick={deleteSelectedDocs} className="is-danger">
                      删除
                    </button>
                    <button type="button" onClick={deselectAllDocs}>
                      取消选择
                    </button>
                  </div>
                </div>
              )}
              <div className="knowledge-sort-bar">
                <div className="knowledge-sort-controls">
                  <span>{sortedDocs.length} 篇文档</span>
                </div>
                <div className="knowledge-sort-controls">
                  <span>排序：</span>
                  <select
                    className="knowledge-sort-select"
                    value={sortBy}
                    onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
                  >
                    <option value="updated">更新时间</option>
                    <option value="title">标题</option>
                    <option value="size">文档大小</option>
                  </select>
                  <button
                    type="button"
                    className="knowledge-sort-order-btn"
                    onClick={() => setSortOrder(o => o === "asc" ? "desc" : "asc")}
                    title={sortOrder === "asc" ? "升序" : "降序"}
                  >
                    <UiIcon name={sortOrder === "asc" ? "chevronDown" : "chevronUp"} size={14} />
                  </button>
                </div>
              </div>
              <div className={batchMode ? "batch-mode" : ""}>
                {sortedDocs.map((doc) => (
                  <article
                    key={doc.id}
                    className={`knowledge-card ${selectedId === doc.id ? "is-active" : ""} ${doc.enabled ? "" : "is-disabled"} ${selectedDocs.has(doc.id) ? "is-selected" : ""}`}
                    onClick={() => batchMode ? toggleDocSelection(doc.id) : setSelectedId(doc.id)}
                  >
                    {batchMode && (
                      <div
                        className={`knowledge-card-select ${selectedDocs.has(doc.id) ? "checked" : ""}`}
                        onClick={(e) => { e.stopPropagation(); toggleDocSelection(doc.id); }}
                      >
                        {selectedDocs.has(doc.id) && <UiIcon name="check" size={12} />}
                      </div>
                    )}
                    <div className="knowledge-card-head">
                      <h2>{doc.title}</h2>
                      <span>{doc.collection}</span>
                    </div>
                    <p>{doc.preview || "空文档"}</p>
                    {doc.tags.length > 0 && (
                      <div className="knowledge-card-tags">
                        {doc.tags.map(tag => (
                          <span key={tag} className="knowledge-tag">{tag}</span>
                        ))}
                      </div>
                    )}
                    <div className="knowledge-card-meta">
                      <span>{doc.chunk_count} 片段</span>
                      <span>{doc.char_count} 字</span>
                      <span>{doc.enabled ? "已启用" : "已停用"}</span>
                    </div>
                  </article>
                ))}
              </div>
            </>
          )}
        </section>

        <section className="knowledge-preview" aria-label="文档预览">
          {detailLoading ? (
            <div className="knowledge-empty" role="status">
              <h2>正在读取原文…</h2>
              <p>正在从知识库正文文件加载内容。</p>
            </div>
          ) : detailError ? (
            <div className="knowledge-empty" role="alert">
              <h2>原文读取失败</h2>
              <p>{detailError}</p>
              <button type="button" onClick={() => setDetailReloadKey((current) => current + 1)}>
                重试
              </button>
            </div>
          ) : detail ? (
            <>
              <div className="knowledge-breadcrumb">
                <button onClick={() => setSelectedId(null)}>知识库</button>
                <UiIcon name="chevronDown" size={10} className="knowledge-breadcrumb-separator" />
                <span>{detail.document.collection}</span>
                <UiIcon name="chevronDown" size={10} className="knowledge-breadcrumb-separator" />
                <span>{detail.document.title}</span>
              </div>
              <div className="knowledge-preview-head">
                <div>
                  <h2>{detail.document.title}</h2>
                  <p className="knowledge-muted">
                    更新于 {formatStamp(detail.document.updated_at)} · {detail.document.source}
                  </p>
                  {detail.document.tags.length > 0 && (
                    <div className="knowledge-card-tags" style={{ marginTop: 8 }}>
                      {detail.document.tags.map(tag => (
                        <span key={tag} className="knowledge-tag">{tag}</span>
                      ))}
                    </div>
                  )}
                </div>
                <div className="knowledge-preview-actions">
                  <button type="button" onClick={editSelected}>
                    <UiIcon name="edit" size={14} />
                    编辑
                  </button>
                  <button type="button" onClick={() => void toggleEnabled(detail.document)} disabled={busy === detail.document.id}>
                    {detail.document.enabled ? "停用" : "启用"}
                  </button>
                  <button type="button" className="is-danger" onClick={() => void deleteDoc(detail.document)}>
                    <UiIcon name="delete" size={14} />
                    删除
                  </button>
                </div>
              </div>
              <div className="knowledge-body">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeHighlight]}
                >
                  {detail.content}
                </ReactMarkdown>
              </div>
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
        <div
          className="knowledge-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="knowledge-editor-title"
          onMouseDown={(event) => {
            if (event.currentTarget === event.target) closeEditor();
          }}
        >
          <div className="knowledge-modal-card" aria-describedby={editorDirty ? "knowledge-unsaved-hint" : undefined}>
            <button
              type="button"
              className="knowledge-modal-close"
              onClick={closeEditor}
              aria-label="关闭"
            >
              <UiIcon name="close" size={16} />
            </button>
            <header>
              <h2 id="knowledge-editor-title">{editor.id ? "编辑笔记" : "新建笔记"}</h2>
              {editorDirty ? <span id="knowledge-unsaved-hint" className="knowledge-unsaved">有未保存修改</span> : null}
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
              <button type="button" onClick={closeEditor}>
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

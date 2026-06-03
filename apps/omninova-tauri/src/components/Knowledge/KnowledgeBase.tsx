import { useCallback, useEffect, useState } from "react";
import { invokeTauri, isTauriEnvironment } from "../../utils/tauri";

export interface KnowledgeDocument {
  id: string;
  filename: string;
  uploaded_at: string;
  sheet_count: number;
  row_count: number;
  chunk_count: number;
  source_path: string;
}

export function KnowledgeBase() {
  const [documents, setDocuments] = useState<KnowledgeDocument[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauriEnvironment()) {
      setError("知识库管理需在桌面应用中打开");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await invokeTauri<KnowledgeDocument[]>("list_knowledge_documents");
      setDocuments(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleUpload = async () => {
    if (!isTauriEnvironment()) return;
    setError(null);
    setStatus(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        title: "选择 Excel 知识库文件",
        filters: [
          {
            name: "Excel",
            extensions: ["xlsx", "xls", "xlsm", "ods"],
          },
        ],
      });
      if (selected == null) return;
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      setLoading(true);
      const result = await invokeTauri<{ document: KnowledgeDocument }>(
        "upload_knowledge_excel",
        { sourcePath: path }
      );
      setStatus(
        `已导入「${result.document.filename}」：${result.document.sheet_count} 个工作表，${result.document.row_count} 行，${result.document.chunk_count} 条检索片段`
      );
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (docId: string, filename: string) => {
    if (!isTauriEnvironment()) return;
    if (!window.confirm(`确定删除知识库文档「${filename}」？`)) return;
    setError(null);
    try {
      await invokeTauri<boolean>("delete_knowledge_document", { docId });
      setStatus(`已删除「${filename}」`);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="setup-embed setup-embed--full">
      <header className="setup-embed-head">
        <h1>外挂知识库</h1>
        <p className="setup-embed-sub">
          上传 Excel（.xlsx / .xls / .xlsm / .ods）作为企业外挂知识源。每行数据会索引为可检索片段；
          对话时 Agent 可使用 <code>knowledge_search</code> 工具查询，并默认将相关片段注入上下文。
          文件保存在工作区 <code>knowledge/</code> 目录。
        </p>
      </header>

      <section className="setup-section">
        <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap", marginBottom: "1rem" }}>
          <button type="button" className="setup-primary-btn" onClick={() => void handleUpload()} disabled={loading}>
            {loading ? "处理中…" : "上传 Excel"}
          </button>
          <button type="button" className="setup-secondary-btn" onClick={() => void refresh()} disabled={loading}>
            刷新列表
          </button>
        </div>
        {status && (
          <p style={{ color: "var(--accent, #4ade80)", fontSize: "0.9rem", margin: "0 0 0.75rem" }}>{status}</p>
        )}
        {error && (
          <p style={{ color: "#f87171", fontSize: "0.9rem", margin: "0 0 0.75rem" }}>{error}</p>
        )}
      </section>

      <section className="setup-section">
        <h2>已导入文档</h2>
        {documents.length === 0 ? (
          <p className="setup-embed-sub" style={{ marginTop: 0 }}>
            暂无文档。点击「上传 Excel」导入价格表、设备台账、规程对照表等。
          </p>
        ) : (
          <div className="setup-grid" style={{ gridTemplateColumns: "1fr" }}>
            {documents.map((doc) => (
              <article
                key={doc.id}
                style={{
                  border: "1px solid rgba(255,255,255,0.08)",
                  borderRadius: 8,
                  padding: "0.85rem 1rem",
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "flex-start",
                  gap: "1rem",
                }}
              >
                <div>
                  <strong>{doc.filename}</strong>
                  <p style={{ margin: "0.35rem 0 0", fontSize: "0.85rem", opacity: 0.85 }}>
                    {doc.sheet_count} 表 · {doc.row_count} 行 · {doc.chunk_count} 片段 · 上传于{" "}
                    {new Date(Number(doc.uploaded_at) * 1000).toLocaleString()}
                  </p>
                  <p style={{ margin: "0.25rem 0 0", fontSize: "0.8rem", opacity: 0.6 }}>
                    ID: <code>{doc.id}</code>
                  </p>
                </div>
                <button
                  type="button"
                  className="setup-secondary-btn"
                  onClick={() => void handleDelete(doc.id, doc.filename)}
                >
                  删除
                </button>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="setup-section">
        <h2>配置说明</h2>
        <p className="setup-embed-sub" style={{ marginTop: 0 }}>
          在 <code>config.toml</code> 的 <code>[knowledge]</code> 段可调整：是否启用、存储目录、自动注入条数、每表最大导入行数。
        </p>
        <pre
          style={{
            margin: 0,
            padding: "0.75rem 1rem",
            borderRadius: 8,
            background: "rgba(0,0,0,0.25)",
            fontSize: "0.8rem",
            overflow: "auto",
          }}
        >
{`[knowledge]
enabled = true
auto_inject = true
auto_inject_limit = 5
max_rows_per_sheet = 10000`}
        </pre>
      </section>
    </div>
  );
}

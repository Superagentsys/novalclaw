use crate::config::{Config, KnowledgeConfig};
use crate::knowledge::excel::{detect_headers, parse_excel_file, row_to_chunk_text};
use crate::memory::search::rank_entries;
use crate::memory::traits::{MemoryCategory, MemoryEntry};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocumentSummary {
    pub id: String,
    pub filename: String,
    pub uploaded_at: String,
    pub sheet_count: usize,
    pub row_count: usize,
    pub chunk_count: usize,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KnowledgeChunk {
    pub id: String,
    pub doc_id: String,
    pub sheet: String,
    pub row_index: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KnowledgeDocument {
    pub id: String,
    pub filename: String,
    pub uploaded_at: String,
    pub sheet_count: usize,
    pub row_count: usize,
    pub source_path: String,
    pub stored_file: String,
    pub chunks: Vec<KnowledgeChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KnowledgeIndex {
    pub documents: Vec<KnowledgeDocument>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeChunkHit {
    pub doc_id: String,
    pub filename: String,
    pub sheet: String,
    pub row_index: usize,
    pub text: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeUploadSummary {
    pub document: KnowledgeDocumentSummary,
}

pub struct KnowledgeStore {
    root: PathBuf,
    options: KnowledgeConfig,
    index: RwLock<KnowledgeIndex>,
}

impl KnowledgeStore {
    pub fn open(config: &Config) -> anyhow::Result<Arc<Self>> {
        let root = knowledge_root(config);
        fs::create_dir_all(root.join("files"))?;
        let index_path = root.join("index.json");
        let index = if index_path.exists() {
            match fs::read_to_string(&index_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                    warn!("knowledge index parse failed: {e}");
                    KnowledgeIndex::default()
                }),
                Err(e) => {
                    warn!("knowledge index read failed: {e}");
                    KnowledgeIndex::default()
                }
            }
        } else {
            KnowledgeIndex::default()
        };
        Ok(Arc::new(Self {
            root,
            options: config.knowledge.clone(),
            index: RwLock::new(index),
        }))
    }

    pub fn is_enabled(&self) -> bool {
        self.options.enabled
    }

    fn save_index(&self) -> anyhow::Result<()> {
        let index = self.index.read();
        let content = serde_json::to_string_pretty(&*index)?;
        fs::create_dir_all(&self.root)?;
        fs::write(self.root.join("index.json"), content)?;
        Ok(())
    }

    pub fn list_documents(&self) -> Vec<KnowledgeDocumentSummary> {
        self.index
            .read()
            .documents
            .iter()
            .map(document_summary)
            .collect()
    }

    pub fn ingest_excel(&self, source_path: &Path) -> anyhow::Result<KnowledgeUploadSummary> {
        if !self.options.enabled {
            anyhow::bail!("外挂知识库未启用，请在配置中设置 [knowledge] enabled = true");
        }
        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "xlsx" && ext != "xls" && ext != "xlsm" && ext != "ods" {
            anyhow::bail!("仅支持 Excel 格式：.xlsx / .xls / .xlsm / .ods");
        }
        if !source_path.exists() {
            anyhow::bail!("文件不存在: {}", source_path.display());
        }

        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload.xlsx")
            .to_string();

        let sheets = parse_excel_file(source_path, self.options.max_rows_per_sheet)?;
        let doc_id = format!("kb-{}", uuid::Uuid::new_v4());
        let uploaded_at = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let stored_name = format!("{doc_id}-{filename}");
        let stored_path = self.root.join("files").join(&stored_name);
        fs::copy(source_path, &stored_path)?;

        let mut chunks = Vec::new();
        let mut row_count = 0usize;
        for sheet in &sheets {
            let headers = sheet.rows.first().and_then(|r| detect_headers(r));
            let data_rows: &[Vec<String>] = if headers.is_some() {
                &sheet.rows[1..]
            } else {
                &sheet.rows[..]
            };
            for (row_index, row) in data_rows.iter().enumerate() {
                row_count += 1;
                let text = row_to_chunk_text(&sheet.name, row_index, row, headers.as_deref());
                chunks.push(KnowledgeChunk {
                    id: format!("{doc_id}-{}-{}", sheet.name, row_index + 1),
                    doc_id: doc_id.clone(),
                    sheet: sheet.name.clone(),
                    row_index: row_index + 1,
                    text,
                });
            }
        }

        let document = KnowledgeDocument {
            id: doc_id.clone(),
            filename: filename.clone(),
            uploaded_at: uploaded_at.clone(),
            sheet_count: sheets.len(),
            row_count,
            source_path: source_path.display().to_string(),
            stored_file: stored_path.display().to_string(),
            chunks,
        };
        let summary = document_summary(&document);

        {
            let mut index = self.index.write();
            index.documents.retain(|d| d.id != doc_id);
            index.documents.push(document);
        }
        self.save_index()?;

        Ok(KnowledgeUploadSummary { document: summary })
    }

    pub fn delete_document(&self, doc_id: &str) -> anyhow::Result<bool> {
        let removed = {
            let mut index = self.index.write();
            let before = index.documents.len();
            if let Some(doc) = index.documents.iter().find(|d| d.id == doc_id) {
                let stored = PathBuf::from(&doc.stored_file);
                if stored.exists() {
                    let _ = fs::remove_file(stored);
                }
            }
            index.documents.retain(|d| d.id != doc_id);
            before != index.documents.len()
        };
        if removed {
            self.save_index()?;
        }
        Ok(removed)
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<KnowledgeChunkHit> {
        let index = self.index.read();
        let mut pseudo: Vec<MemoryEntry> = Vec::new();
        for doc in &index.documents {
            for chunk in &doc.chunks {
                pseudo.push(MemoryEntry {
                    id: chunk.id.clone(),
                    key: format!("{}:{}:{}", doc.filename, chunk.sheet, chunk.row_index),
                    content: chunk.text.clone(),
                    category: MemoryCategory::Custom("knowledge".into()),
                    timestamp: doc.uploaded_at.clone(),
                    session_id: Some(doc.id.clone()),
                    score: None,
                });
            }
        }
        if pseudo.is_empty() {
            return Vec::new();
        }
        let ranked = rank_entries(query, pseudo);
        ranked
            .into_iter()
            .take(limit)
            .filter_map(|entry| {
                let doc_id = entry.session_id?;
                let doc = index.documents.iter().find(|d| d.id == doc_id)?;
                let chunk = doc.chunks.iter().find(|c| c.id == entry.id)?;
                Some(KnowledgeChunkHit {
                    doc_id: doc.id.clone(),
                    filename: doc.filename.clone(),
                    sheet: chunk.sheet.clone(),
                    row_index: chunk.row_index,
                    text: chunk.text.clone(),
                    score: entry.score,
                })
            })
            .collect()
    }

    pub fn format_context_block(&self, query: &str, limit: usize) -> String {
        let hits = self.search(query, limit);
        if hits.is_empty() {
            return String::new();
        }
        let mut lines = vec![
            "## 外挂知识库（Excel 表格）".to_string(),
            "以下片段来自用户上传的 Excel，回答时请优先依据这些内容；若与常识冲突，以表格为准。".to_string(),
        ];
        for (i, hit) in hits.iter().enumerate() {
            lines.push(format!(
                "\n### 片段 {} — {} / {} / 第{}行\n{}",
                i + 1,
                hit.filename,
                hit.sheet,
                hit.row_index,
                hit.text
            ));
        }
        lines.join("\n")
    }

    pub fn document_count(&self) -> usize {
        self.index.read().documents.len()
    }

    pub fn chunk_count(&self) -> usize {
        self.index
            .read()
            .documents
            .iter()
            .map(|d| d.chunks.len())
            .sum()
    }
}

fn document_summary(doc: &KnowledgeDocument) -> KnowledgeDocumentSummary {
    KnowledgeDocumentSummary {
        id: doc.id.clone(),
        filename: doc.filename.clone(),
        uploaded_at: doc.uploaded_at.clone(),
        sheet_count: doc.sheet_count,
        row_count: doc.row_count,
        chunk_count: doc.chunks.len(),
        source_path: doc.source_path.clone(),
    }
}

pub fn knowledge_root(config: &Config) -> PathBuf {
    config
        .knowledge
        .dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join("knowledge"))
}

//! Disk-backed knowledge index shared by desktop, Gateway, and CLI.

use super::chunk::{chunk_text, Chunk};
use crate::cron::now_timestamp;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_DOC_CHARS: usize = 1_500_000;
const PREVIEW_CHARS: usize = 280;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    #[serde(default = "default_collection")]
    pub collection: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub char_count: usize,
    #[serde(default)]
    pub chunk_count: usize,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_collection() -> String {
    "default".into()
}
fn default_source() -> String {
    "note".into()
}
fn default_kind() -> String {
    "md".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeHit {
    pub document_id: String,
    pub title: String,
    pub collection: String,
    pub chunk_index: usize,
    pub heading: Option<String>,
    pub snippet: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct KnowledgeUpsert {
    pub id: Option<String>,
    pub title: String,
    pub collection: String,
    pub source: String,
    pub source_path: Option<String>,
    pub kind: String,
    pub tags: Vec<String>,
    pub content: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreFile {
    #[serde(default)]
    documents: Vec<KnowledgeDocument>,
    #[serde(default)]
    chunks: Vec<StoredChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredChunk {
    document_id: String,
    index: usize,
    heading: Option<String>,
    text: String,
}

#[derive(Clone)]
pub struct KnowledgeStore {
    index_path: PathBuf,
    docs_dir: PathBuf,
    write_guard: Arc<Mutex<()>>,
}

impl KnowledgeStore {
    pub async fn open_in(workspace: impl AsRef<Path>) -> Result<Self> {
        let root = workspace.as_ref().join("knowledge");
        let docs_dir = root.join("docs");
        tokio::fs::create_dir_all(&docs_dir).await?;
        Ok(Self {
            index_path: root.join("index.json"),
            docs_dir,
            write_guard: Arc::new(Mutex::new(())),
        })
    }

    pub async fn list(&self, collection: Option<&str>) -> Vec<KnowledgeDocument> {
        let file = self.read_all().await;
        let mut docs = file.documents;
        if let Some(collection) = collection.filter(|c| !c.is_empty() && *c != "all") {
            docs.retain(|doc| doc.collection == collection);
        }
        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        docs
    }

    pub async fn collections(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .read_all()
            .await
            .documents
            .into_iter()
            .map(|doc| doc.collection)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub async fn get(&self, id: &str) -> Result<Option<(KnowledgeDocument, String)>> {
        let StoreFile { documents, chunks } = self.read_all().await;
        let Some(doc) = documents.into_iter().find(|doc| doc.id == id) else {
            return Ok(None);
        };
        let content = match self.read_body(&doc.id).await {
            Ok(content) if !content.is_empty() || doc.char_count == 0 => content,
            _ => {
                let recovered = self.recover_body(&doc, &chunks).await?;
                // Older knowledge indexes did not always persist `{id}.md`. Repair the
                // canonical body as soon as we can recover it, so later reads and edits
                // no longer depend on the compatibility path.
                self.write_body(&doc.id, &recovered).await?;
                recovered
            }
        };
        Ok(Some((doc, content)))
    }

    pub async fn upsert(&self, input: KnowledgeUpsert) -> Result<KnowledgeDocument> {
        let _guard = self.write_guard.lock().await;
        let mut file = self.read_all().await;
        let content = clamp_content(&input.content);
        let chunks = chunk_text(&content);
        let existing = input
            .id
            .as_ref()
            .filter(|id| !id.is_empty())
            .and_then(|id| file.documents.iter().find(|doc| doc.id == *id).cloned());
        let id = existing
            .as_ref()
            .map(|doc| doc.id.clone())
            .unwrap_or_else(new_doc_id);
        let now = now_timestamp();
        let doc = KnowledgeDocument {
            id: id.clone(),
            title: nonempty_title(&input.title, &input.source_path),
            collection: nonempty_collection(&input.collection),
            source: if input.source.trim().is_empty() {
                "note".into()
            } else {
                input.source
            },
            source_path: input.source_path,
            kind: if input.kind.trim().is_empty() {
                "md".into()
            } else {
                input.kind
            },
            tags: input.tags,
            preview: preview_of(&content),
            char_count: content.chars().count(),
            chunk_count: chunks.len(),
            enabled: input.enabled,
            created_at: existing
                .as_ref()
                .map(|doc| doc.created_at.clone())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        file.documents.retain(|item| item.id != id);
        file.chunks.retain(|chunk| chunk.document_id != id);
        file.chunks.extend(stored_chunks(&id, &chunks));
        file.documents.push(doc.clone());
        self.write_body(&id, &content).await?;
        self.write_all(&file).await?;
        Ok(doc)
    }

    pub async fn import_path(
        &self,
        path: &Path,
        collection: Option<&str>,
        tags: Vec<String>,
    ) -> Result<KnowledgeDocument> {
        let content = extract_file_text(path).await?;
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled")
            .to_string();
        let kind = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("txt")
            .to_ascii_lowercase();
        self.upsert(KnowledgeUpsert {
            id: None,
            title,
            collection: collection.unwrap_or("default").to_string(),
            source: "file".into(),
            source_path: Some(path.display().to_string()),
            kind,
            tags,
            content,
            enabled: true,
        })
        .await
    }

    pub async fn import_bytes(
        &self,
        filename: &str,
        bytes: &[u8],
        collection: Option<&str>,
        tags: Vec<String>,
    ) -> Result<KnowledgeDocument> {
        let kind = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("txt")
            .to_ascii_lowercase();
        let content = if kind == "pdf" {
            extract_pdf_bytes(bytes)?
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
        let title = Path::new(filename)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled")
            .to_string();
        self.upsert(KnowledgeUpsert {
            id: None,
            title,
            collection: collection.unwrap_or("default").to_string(),
            source: "upload".into(),
            source_path: Some(filename.to_string()),
            kind,
            tags,
            content,
            enabled: true,
        })
        .await
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<Option<KnowledgeDocument>> {
        let _guard = self.write_guard.lock().await;
        let mut file = self.read_all().await;
        let Some(doc) = file.documents.iter_mut().find(|doc| doc.id == id) else {
            return Ok(None);
        };
        doc.enabled = enabled;
        doc.updated_at = now_timestamp();
        let cloned = doc.clone();
        self.write_all(&file).await?;
        Ok(Some(cloned))
    }

    pub async fn remove(&self, id: &str) -> Result<bool> {
        let _guard = self.write_guard.lock().await;
        let mut file = self.read_all().await;
        let before = file.documents.len();
        file.documents.retain(|doc| doc.id != id);
        file.chunks.retain(|chunk| chunk.document_id != id);
        if file.documents.len() == before {
            return Ok(false);
        }
        self.write_all(&file).await?;
        let body = self.docs_dir.join(format!("{id}.md"));
        let _ = tokio::fs::remove_file(body).await;
        Ok(true)
    }

    pub async fn search(
        &self,
        query: &str,
        collection: Option<&str>,
        limit: usize,
    ) -> Vec<KnowledgeHit> {
        let file = self.read_all().await;
        let enabled: std::collections::HashMap<_, _> = file
            .documents
            .iter()
            .filter(|doc| doc.enabled)
            .filter(|doc| match collection {
                Some(name) if !name.is_empty() && name != "all" => doc.collection == name,
                _ => true,
            })
            .map(|doc| (doc.id.clone(), doc.clone()))
            .collect();
        let mut scored: Vec<KnowledgeHit> = file
            .chunks
            .into_iter()
            .filter_map(|chunk| {
                let doc = enabled.get(&chunk.document_id)?;
                let score = score_chunk(query, doc, &chunk);
                if score <= 0.0 {
                    return None;
                }
                Some(KnowledgeHit {
                    document_id: doc.id.clone(),
                    title: doc.title.clone(),
                    collection: doc.collection.clone(),
                    chunk_index: chunk.index,
                    heading: chunk.heading,
                    snippet: snippet_around(&chunk.text, query),
                    score,
                })
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.title.cmp(&b.title))
        });
        scored.truncate(limit.max(1).min(50));
        scored
    }

    pub async fn catalog_prompt(&self) -> String {
        let docs = self.list(None).await;
        let enabled: Vec<_> = docs.into_iter().filter(|doc| doc.enabled).collect();
        if enabled.is_empty() {
            return String::new();
        }
        let mut lines = vec![
            "\n## Knowledge base".to_string(),
            "A local document library is available. Call `knowledge_search` to retrieve passages instead of guessing.".to_string(),
        ];
        let mut collections = enabled
            .iter()
            .map(|doc| doc.collection.clone())
            .collect::<Vec<_>>();
        collections.sort();
        collections.dedup();
        lines.push(format!("Collections: {}.", collections.join(", ")));
        lines.push("Documents:".into());
        for doc in enabled.iter().take(40) {
            let tags = if doc.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", doc.tags.join(", "))
            };
            lines.push(format!(
                "- {} ({}, {} chars){}",
                doc.title, doc.collection, doc.char_count, tags
            ));
        }
        if enabled.len() > 40 {
            lines.push(format!("- … {} more", enabled.len() - 40));
        }
        lines.join("\n")
    }

    async fn read_all(&self) -> StoreFile {
        match tokio::fs::read_to_string(&self.index_path).await {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => StoreFile::default(),
        }
    }

    async fn write_all(&self, file: &StoreFile) -> Result<()> {
        if let Some(parent) = self.index_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(file)?;
        tokio::fs::write(&self.index_path, json).await?;
        Ok(())
    }

    async fn read_body(&self, id: &str) -> Result<String> {
        Ok(tokio::fs::read_to_string(self.docs_dir.join(format!("{id}.md"))).await?)
    }

    async fn recover_body(
        &self,
        doc: &KnowledgeDocument,
        chunks: &[StoredChunk],
    ) -> Result<String> {
        // Compatibility with early/demo indexes that stored `<title>.md` instead of
        // the canonical `<id>.md`. Scan file stems rather than joining an untrusted
        // title into a path.
        if let Ok(mut entries) = tokio::fs::read_dir(&self.docs_dir).await {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.file_stem().and_then(|stem| stem.to_str()) == Some(doc.title.as_str()) {
                    if let Ok(content) = tokio::fs::read_to_string(&path).await {
                        if !content.is_empty() || doc.char_count == 0 {
                            return Ok(content);
                        }
                    }
                }
            }
        }

        let recovered = reconstruct_content(
            chunks
                .iter()
                .filter(|chunk| chunk.document_id == doc.id)
                .collect(),
        );
        if !recovered.is_empty() || doc.char_count == 0 {
            return Ok(recovered);
        }

        Err(anyhow!(
            "knowledge document body is missing and cannot be recovered: {} ({})",
            doc.title,
            doc.id
        ))
    }

    async fn write_body(&self, id: &str, content: &str) -> Result<()> {
        tokio::fs::create_dir_all(&self.docs_dir).await?;
        tokio::fs::write(self.docs_dir.join(format!("{id}.md")), content).await?;
        Ok(())
    }
}

fn reconstruct_content(mut chunks: Vec<&StoredChunk>) -> String {
    chunks.sort_by_key(|chunk| chunk.index);
    let mut content = String::new();
    let mut previous_heading: Option<&str> = None;

    for chunk in chunks {
        let heading = chunk
            .heading
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if heading != previous_heading {
            if let Some(heading) = heading {
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str("# ");
                content.push_str(heading);
                content.push_str("\n\n");
            }
            previous_heading = heading;
        }
        append_without_overlap(&mut content, chunk.text.trim());
    }

    content.trim().to_string()
}

fn append_without_overlap(content: &mut String, next: &str) {
    if next.is_empty() {
        return;
    }
    if content.is_empty() {
        content.push_str(next);
        return;
    }

    let existing_chars: Vec<char> = content.chars().collect();
    let next_chars: Vec<char> = next.chars().collect();
    let max_overlap = existing_chars.len().min(next_chars.len()).min(512);
    let overlap = (1..=max_overlap)
        .rev()
        .find(|count| existing_chars[existing_chars.len() - count..] == next_chars[..*count])
        .unwrap_or(0);

    if overlap == 0 {
        content.push_str("\n\n");
    }
    content.extend(next_chars[overlap..].iter());
}

pub async fn append_knowledge_prompt(system_prompt: &mut Option<String>, workspace: &Path) {
    let Ok(store) = KnowledgeStore::open_in(workspace).await else {
        return;
    };
    let extra = store.catalog_prompt().await;
    if extra.is_empty() {
        return;
    }
    let current = system_prompt.take().unwrap_or_default();
    *system_prompt = Some(format!("{current}\n{extra}"));
}

fn stored_chunks(id: &str, chunks: &[Chunk]) -> Vec<StoredChunk> {
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| StoredChunk {
            document_id: id.to_string(),
            index,
            heading: chunk.heading.clone(),
            text: chunk.text.clone(),
        })
        .collect()
}

fn clamp_content(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= MAX_DOC_CHARS {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_DOC_CHARS).collect()
}

fn preview_of(content: &str) -> String {
    let collapsed: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= PREVIEW_CHARS {
        collapsed
    } else {
        let mut preview: String = collapsed.chars().take(PREVIEW_CHARS).collect();
        preview.push('…');
        preview
    }
}

fn nonempty_title(title: &str, source_path: &Option<String>) -> String {
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    source_path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem()?.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn nonempty_collection(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "default".into()
    } else {
        trimmed.to_string()
    }
}

fn new_doc_id() -> String {
    format!("doc-{}", now_timestamp().replace([':', '.'], "-"))
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut latin = String::new();
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if is_cjk(ch) {
            flush_latin_token(&mut latin, &mut tokens);
            if index + 1 < chars.len() && is_cjk(chars[index + 1]) {
                tokens.push(format!("{}{}", ch, chars[index + 1]));
            }
            index += 1;
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            latin.push(ch);
            index += 1;
            continue;
        }
        flush_latin_token(&mut latin, &mut tokens);
        index += 1;
    }
    flush_latin_token(&mut latin, &mut tokens);
    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
    )
}

fn flush_latin_token(latin: &mut String, tokens: &mut Vec<String>) {
    if latin.is_empty() {
        return;
    }
    let token = latin.to_lowercase();
    latin.clear();
    if token.chars().count() >= 2 {
        tokens.push(token);
    }
}

fn score_chunk(query: &str, doc: &KnowledgeDocument, chunk: &StoredChunk) -> f64 {
    let query_norm = query.trim().to_lowercase();
    if query_norm.is_empty() {
        return 0.0;
    }
    let tokens = tokenize(&query_norm);
    let hay_title = doc.title.to_lowercase();
    let hay_heading = chunk.heading.clone().unwrap_or_default().to_lowercase();
    let hay_text = chunk.text.to_lowercase();
    let mut score = 0.0;
    if hay_title.contains(&query_norm) {
        score += 8.0;
    }
    if hay_heading.contains(&query_norm) {
        score += 5.0;
    }
    if hay_text.contains(&query_norm) {
        score += 4.0;
    }
    for token in &tokens {
        if hay_title.contains(token) {
            score += 2.5;
        }
        if hay_heading.contains(token) {
            score += 1.5;
        }
        if hay_text.contains(token) {
            score += 1.0;
        }
    }
    score
}

fn snippet_around(text: &str, query: &str) -> String {
    let lower = text.to_lowercase();
    let needle = query.trim().to_lowercase();
    let byte_idx = if needle.is_empty() {
        0
    } else {
        lower.find(&needle).unwrap_or_else(|| {
            tokenize(&needle)
                .into_iter()
                .find_map(|token| lower.find(&token))
                .unwrap_or(0)
        })
    };
    let char_start = lower
        .char_indices()
        .position(|(offset, _)| offset >= byte_idx)
        .unwrap_or(0);
    let start = char_start.saturating_sub(80);
    let snippet: String = text.chars().skip(start).take(240).collect();
    let mut out = snippet.trim().to_string();
    if start > 0 {
        out = format!("…{out}");
    }
    if text.chars().count() > start + 240 {
        out.push('…');
    }
    out
}

async fn extract_file_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "pdf" {
        let path = path.to_path_buf();
        let text = tokio::task::spawn_blocking(move || pdf_extract::extract_text(&path))
            .await
            .context("pdf extract task")?
            .context("pdf extract")?;
        return Ok(text);
    }
    Ok(tokio::fs::read_to_string(path).await?)
}

fn extract_pdf_bytes(bytes: &[u8]) -> Result<String> {
    let dir = std::env::temp_dir().join("omninova-knowledge");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "upload-{}.pdf",
        now_timestamp().replace([':', '.'], "-")
    ));
    std::fs::write(&path, bytes)?;
    let text = pdf_extract::extract_text(&path).context("pdf extract")?;
    let _ = std::fs::remove_file(&path);
    Ok(text)
}

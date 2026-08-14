//! Local document knowledge base: ingest, chunk, and keyword search.
//!
//! Documents live under `{workspace}/knowledge/docs/{id}.md`. Search metadata
//! and chunks are in `{workspace}/knowledge/index.json` so the desktop UI,
//! Gateway, and CLI share one store without a vector database.

mod chunk;
mod store;

pub use chunk::{chunk_text, Chunk};
pub use store::{
    append_knowledge_prompt, KnowledgeDocument, KnowledgeHit, KnowledgeStore, KnowledgeUpsert,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_markdown_sections_and_long_paragraphs() {
        let text = "# Intro\n\nshort\n\n# Details\n\n".to_string() + &"word ".repeat(400);
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("Intro"));
        assert!(chunks.iter().any(|c| c.heading.as_deref() == Some("Details")));
    }

    #[tokio::test]
    async fn stores_and_searches_a_note() {
        let dir = std::env::temp_dir().join(format!(
            "omninova-kb-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.expect("tempdir");
        let store = KnowledgeStore::open_in(&dir).await.expect("open");
        let doc = store
            .upsert(KnowledgeUpsert {
                id: None,
                title: "Onboarding".into(),
                collection: "ops".into(),
                source: "note".into(),
                source_path: None,
                kind: "md".into(),
                tags: vec!["hr".into()],
                content: "The office wifi password is aurora-7. New hires ask IT.".into(),
                enabled: true,
            })
            .await
            .expect("upsert");
        assert_eq!(doc.chunk_count, 1);
        let hits = store.search("wifi password", None, 5).await;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.to_lowercase().contains("aurora"));
        assert_eq!(hits[0].title, "Onboarding");

        let listed = store.list(Some("ops")).await;
        assert_eq!(listed.len(), 1);
        store.remove(&doc.id).await.expect("remove");
        assert!(store.list(None).await.is_empty());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

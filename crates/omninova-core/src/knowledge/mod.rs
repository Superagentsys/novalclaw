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
        assert!(chunks
            .iter()
            .any(|c| c.heading.as_deref() == Some("Details")));
    }

    #[test]
    fn keeps_hash_prefix_lines_in_searchable_body() {
        let text = [
            "# Guide",
            "",
            "#include <stdio.h>",
            "#define MAX 8",
            "#!/usr/bin/env bash",
            "# 这是紧挨着代码的脚本注释",
            "echo hi",
            "#话题标签不要当标题",
            "# include still looks like C",
        ]
        .join("\n");
        let chunks = chunk_text(&text);
        let blob: String = chunks
            .iter()
            .map(|chunk| {
                format!(
                    "{} {}",
                    chunk.heading.clone().unwrap_or_default(),
                    chunk.text
                )
            })
            .collect();
        assert!(chunks.iter().any(|c| c.heading.as_deref() == Some("Guide")));
        assert!(blob.contains("#include <stdio.h>"));
        assert!(blob.contains("#define MAX 8"));
        assert!(blob.contains("#!/usr/bin/env bash"));
        assert!(blob.contains("#话题标签不要当标题"));
        assert!(blob.contains("# 这是紧挨着代码的脚本注释"));
        assert!(blob.contains("echo hi"));
        assert!(
            !chunks
                .iter()
                .any(|c| c.heading.as_deref() == Some("include <stdio.h>"))
        );
        assert!(
            !chunks
                .iter()
                .any(|c| c.heading.as_deref() == Some("话题标签不要当标题"))
        );
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

    #[tokio::test]
    async fn recovers_legacy_and_missing_document_bodies() {
        let dir = std::env::temp_dir().join(format!(
            "omninova-kb-recovery-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.expect("tempdir");
        let store = KnowledgeStore::open_in(&dir).await.expect("open");
        let content = format!(
            "# Guide\n\n{}\n\nTHE-END",
            "important instructions ".repeat(180)
        );
        let doc = store
            .upsert(KnowledgeUpsert {
                id: None,
                title: "Legacy Guide".into(),
                collection: "ops".into(),
                source: "note".into(),
                source_path: None,
                kind: "md".into(),
                tags: Vec::new(),
                content: content.clone(),
                enabled: true,
            })
            .await
            .expect("upsert");

        let canonical = dir
            .join("knowledge")
            .join("docs")
            .join(format!("{}.md", doc.id));
        let legacy = dir.join("knowledge").join("docs").join("Legacy Guide.md");
        tokio::fs::rename(&canonical, &legacy)
            .await
            .expect("move to legacy title path");
        let (_, legacy_content) = store
            .get(&doc.id)
            .await
            .expect("get legacy")
            .expect("legacy document");
        assert_eq!(legacy_content, content.trim());
        assert!(
            canonical.exists(),
            "legacy read should repair the canonical body"
        );

        tokio::fs::remove_file(&canonical)
            .await
            .expect("remove canonical");
        tokio::fs::remove_file(&legacy)
            .await
            .expect("remove legacy");
        let (_, reconstructed) = store
            .get(&doc.id)
            .await
            .expect("get reconstructed")
            .expect("reconstructed document");
        assert!(reconstructed.starts_with("# Guide"));
        assert!(reconstructed.ends_with("THE-END"));
        assert!(
            canonical.exists(),
            "chunk recovery should repair the canonical body"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

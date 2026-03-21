pub mod backend;
pub mod embedding;
pub mod episodic;
pub mod factory;
pub mod manager;
pub mod metrics;
pub mod semantic;
pub mod traits;
pub mod file_store;
pub mod lru;
pub mod working;

pub use traits::{Memory, MemoryCategory, MemoryEntry as TraitMemoryEntry};
pub use file_store::FileMemory;
pub use lru::LruMemory;
pub use working::{WorkingMemory, WorkingMemoryEntry, MemoryStats};
pub use episodic::{
    EpisodicMemory, EpisodicMemoryStore, EpisodicMemoryUpdate, NewEpisodicMemory,
    EpisodicMemoryStats,
};
pub use embedding::{
    EmbeddingService, cosine_similarity,
    DEFAULT_EMBEDDING_DIM, DEFAULT_OPENAI_EMBEDDING_MODEL, DEFAULT_OLLAMA_EMBEDDING_MODEL,
};
pub use semantic::{
    SemanticMemory, SemanticMemoryStore, SemanticMemoryStats, SemanticSearchResult,
    NewSemanticMemory,
};
pub use manager::{
    MemoryManager, MemoryLayer, MemoryQuery, MemoryQueryResult, MemoryManagerStats,
    UnifiedMemoryEntry, EvictionPolicy, BenchmarkResults,
};
pub use metrics::{
    MetricsCollector, PerformanceStats, QueryMetric, QueryType, QueryTimer,
};

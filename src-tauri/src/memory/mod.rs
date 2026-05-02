pub mod embedding;
pub mod episodic;
pub mod semantic;

pub use embedding::EmbeddingService;
pub use episodic::EpisodicStore;
pub use semantic::SemanticStore;

use anyhow::Result;
use std::sync::Arc;

/// Single entry point for all memory operations.
/// Both stores share one LanceDB connection to the same database file.
pub struct MemoryStore {
    pub episodic: EpisodicStore,
    pub semantic: SemanticStore,
    pub embedding: EmbeddingService,
}

impl MemoryStore {
    pub async fn episodic_count(&self) -> u64 {
        self.episodic.count().await.unwrap_or(0)
    }

    pub async fn semantic_count(&self) -> u64 {
        self.semantic.count().await.unwrap_or(0)
    }

    pub async fn open(db_path: &str, embed_port: u16) -> Result<Self> {
        let conn = Arc::new(lancedb::connect(db_path).execute().await?);
        Ok(Self {
            episodic: EpisodicStore::new(conn.clone()).await?,
            semantic: SemanticStore::new(conn).await?,
            embedding: EmbeddingService::new(embed_port),
        })
    }
}

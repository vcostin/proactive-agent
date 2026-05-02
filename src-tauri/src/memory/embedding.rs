use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Embedding dimension for nomic-embed-text v1.
/// If you switch embedding models, update this constant and wipe the DB —
/// existing vectors will be incompatible.
pub const EMBED_DIM: usize = 768;

pub struct EmbeddingService {
    client: Client,
    base_url: String,
    last_latency_ms: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

impl EmbeddingService {
    pub fn new(port: u16) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Embed `text` using the locked nomic-embed-text model.
    /// Returns a EMBED_DIM-dimensional vector.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let start = std::time::Instant::now();

        let req = EmbedRequest { input: text, model: "nomic-embed-text" };
        let resp: EmbedResponse = self
            .client
            .post(format!("{}/v1/embeddings", self.base_url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.last_latency_ms
            .store(start.elapsed().as_millis() as u64, Ordering::Relaxed);

        resp.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow::anyhow!("empty embedding response from llama.cpp"))
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

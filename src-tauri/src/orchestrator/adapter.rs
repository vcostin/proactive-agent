use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::context::{AssembledContext, Message};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub tokens_per_sec: f64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[async_trait]
pub trait ModelAdapter: Send + Sync {
    async fn complete(&self, context: AssembledContext) -> Result<ModelResponse>;
    fn model_id(&self) -> &str;
}

pub struct LlamaCppAdapter {
    client: Client,
    base_url: String,
    /// Fallback alias (passed via --alias when starting the server).
    fallback_alias: String,
    /// Discovered model ID from GET /v1/models — cached after first successful query.
    discovered_id: Arc<RwLock<Option<String>>>,
}

impl LlamaCppAdapter {
    pub fn new(port: u16, fallback_alias: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            fallback_alias: fallback_alias.into(),
            discovered_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Query GET /v1/models and return the first model ID the server reports.
    /// Caches the result so subsequent calls are instant.
    /// Falls back to the configured alias if the query fails.
    async fn resolve_model_id(&self) -> String {
        // Fast path: return cached ID
        if let Some(ref id) = *self.discovered_id.read().await {
            return id.clone();
        }

        // Query the server for its actual model list
        #[derive(Deserialize)]
        struct ModelsResp { data: Vec<ModelEntry> }
        #[derive(Deserialize)]
        struct ModelEntry { id: String }

        let discovered = self.client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .ok()
            .and_then(|r| r.error_for_status().ok())
            .and_then(|r| {
                // Parse synchronously from bytes to avoid nested async
                tokio::task::block_in_place(|| {
                    futures::executor::block_on(r.json::<ModelsResp>()).ok()
                })
            })
            .and_then(|m| m.data.into_iter().next())
            .map(|m| m.id);

        if let Some(ref id) = discovered {
            *self.discovered_id.write().await = Some(id.clone());
            return id.clone();
        }

        self.fallback_alias.clone()
    }
}

// ── OpenAI-compatible wire types ─────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<Usage>,
    timings: Option<Timings>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: WireMessage,
}

#[derive(Deserialize)]
struct WireMessage {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct Timings {
    predicted_per_second: Option<f64>,
}

// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelAdapter for LlamaCppAdapter {
    async fn complete(&self, context: AssembledContext) -> Result<ModelResponse> {
        let model_id = self.resolve_model_id().await;

        let req = ChatRequest {
            model: &model_id,
            messages: context.to_messages(),
            stream: false,
        };

        let resp: ChatResponse = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        let tokens_per_sec =
            resp.timings.and_then(|t| t.predicted_per_second).unwrap_or(0.0);

        let (prompt_tokens, completion_tokens) =
            resp.usage.map(|u| (u.prompt_tokens, u.completion_tokens)).unwrap_or((0, 0));

        Ok(ModelResponse { content, tokens_per_sec, prompt_tokens, completion_tokens })
    }

    fn model_id(&self) -> &str {
        &self.fallback_alias
    }
}

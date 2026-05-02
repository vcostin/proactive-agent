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
    fallback_alias: String,
    /// Model ID discovered from GET /v1/models — populated on first use.
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

    /// Ask the server what model ID it actually uses.
    /// Fully async — no block_in_place or block_on.
    async fn discover_model_id(&self) -> Option<String> {
        #[derive(Deserialize)]
        struct ModelsResp {
            data: Vec<ModelEntry>,
        }
        #[derive(Deserialize)]
        struct ModelEntry {
            id: String,
        }

        let resp = self
            .client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;

        let models = resp.json::<ModelsResp>().await.ok()?;
        let id = models.data.into_iter().next().map(|m| m.id)?;
        eprintln!("[ADAPTER] /v1/models → discovered model id: '{id}'");
        Some(id)
    }

    /// Return the model ID to use in API calls — cached after the first query.
    async fn resolve_model_id(&self) -> String {
        // Fast path
        {
            let guard = self.discovered_id.read().await;
            if let Some(ref id) = *guard {
                return id.clone();
            }
        }

        // Query and cache
        if let Some(id) = self.discover_model_id().await {
            *self.discovered_id.write().await = Some(id.clone());
            return id;
        }

        // Fall back to the configured alias
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

        eprintln!("[ADAPTER] POST /v1/chat/completions with model='{model_id}'");

        let raw = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await?;

        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_else(|_| "<empty>".to_string());
            // Reset cache so we re-discover on next attempt
            *self.discovered_id.write().await = None;
            return Err(anyhow::anyhow!(
                "model='{}' → {} — server said: {}",
                model_id,
                status,
                body.chars().take(300).collect::<String>()
            ));
        }

        let resp: ChatResponse = raw.json().await?;

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

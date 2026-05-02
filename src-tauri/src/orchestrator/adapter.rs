use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
    model: String,
}

impl LlamaCppAdapter {
    /// `model_alias` must match the `--alias` flag passed to llama-server.
    /// We use "llama-chat" for the chat server so the ID is always predictable
    /// regardless of what model file is loaded.
    pub fn new(port: u16, model_alias: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            model: model_alias.into(),
        }
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
    /// llama.cpp-specific timing block — not part of the OpenAI spec
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
        let req = ChatRequest {
            model: &self.model,
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
        &self.model
    }
}

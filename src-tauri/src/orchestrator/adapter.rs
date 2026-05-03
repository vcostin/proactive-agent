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

    async fn discover_model_id(&self) -> Option<String> {
        let raw = match self.client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => { eprintln!("[ADAPTER] GET /v1/models error: {e}"); return None; }
        };

        let status = raw.status();
        let body = raw.text().await.unwrap_or_default();
        eprintln!("[ADAPTER] GET /v1/models → {status} — {}", &body[..body.len().min(300)]);
        if !status.is_success() { return None; }

        // llama.cpp server uses {"models":[{"model":"..."}]}
        // OpenAI format uses {"data":[{"id":"..."}]} — handle both
        #[derive(Deserialize)]
        struct ModelsResp {
            models: Option<Vec<LlamaEntry>>,
            data:   Option<Vec<OaiEntry>>,
        }
        #[derive(Deserialize)]
        struct LlamaEntry { model: String }
        #[derive(Deserialize)]
        struct OaiEntry { id: String }

        match serde_json::from_str::<ModelsResp>(&body) {
            Ok(m) => {
                let id = m.models.and_then(|v| v.into_iter().next().map(|e| e.model))
                    .or_else(|| m.data.and_then(|v| v.into_iter().next().map(|e| e.id)));
                if let Some(ref i) = id { eprintln!("[ADAPTER] discovered model id: '{i}'"); }
                id
            }
            Err(e) => { eprintln!("[ADAPTER] /v1/models parse error: {e}"); None }
        }
    }

    async fn resolve_model_id(&self) -> String {
        {
            let guard = self.discovered_id.read().await;
            if let Some(ref id) = *guard { return id.clone(); }
        }
        if let Some(id) = self.discover_model_id().await {
            *self.discovered_id.write().await = Some(id.clone());
            return id;
        }
        self.fallback_alias.clone()
    }

    /// Try OpenAI-compatible /v1/chat/completions.
    /// Returns Ok(None) on 404 so the caller can fall back.
    async fn try_v1_chat(
        &self,
        model_id: &str,
        messages: Vec<Message>,
    ) -> Result<Option<ModelResponse>> {
        #[derive(Serialize)]
        struct ChatReq<'a> { model: &'a str, messages: Vec<Message>, stream: bool }
        #[derive(Deserialize)]
        struct ChatResp { choices: Vec<Choice>, usage: Option<Usage>, timings: Option<Timings> }
        #[derive(Deserialize)]
        struct Choice { message: WireMsg }
        #[derive(Deserialize)]
        struct WireMsg { content: String }
        #[derive(Deserialize)]
        struct Usage { prompt_tokens: u32, completion_tokens: u32 }
        #[derive(Deserialize)]
        struct Timings { predicted_per_second: Option<f64> }

        let req = ChatReq { model: model_id, messages, stream: false };
        let raw = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await?;

        if raw.status().as_u16() == 404 {
            let body = raw.text().await.unwrap_or_default();
            eprintln!("[ADAPTER] /v1/chat/completions 404 — {}", &body[..body.len().min(200)]);
            return Ok(None); // signal to try fallback
        }

        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_default();
            *self.discovered_id.write().await = None;
            return Err(anyhow::anyhow!("model='{}' → {} — {}", model_id, status, &body[..body.len().min(300)]));
        }

        let resp: ChatResp = raw.json().await?;
        let content = resp.choices.into_iter().next().map(|c| c.message.content).unwrap_or_default();
        let tps = resp.timings.and_then(|t| t.predicted_per_second).unwrap_or(0.0);
        let (pt, ct) = resp.usage.map(|u| (u.prompt_tokens, u.completion_tokens)).unwrap_or((0, 0));
        Ok(Some(ModelResponse { content, tokens_per_sec: tps, prompt_tokens: pt, completion_tokens: ct }))
    }

    /// Native llama.cpp POST /completion endpoint — no model ID required.
    /// Falls back to this when /v1/ routes are not available (common in some builds).
    async fn native_completion(&self, messages: Vec<Message>) -> Result<ModelResponse> {
        #[derive(Serialize)]
        struct NativeReq<'a> { prompt: String, stop: &'a [&'a str], stream: bool }
        #[derive(Deserialize)]
        struct NativeResp { content: String, timings: Option<NativeTimings> }
        #[derive(Deserialize)]
        struct NativeTimings { predicted_per_second: Option<f64> }

        let prompt = format_llama3_prompt(&messages);
        eprintln!("[ADAPTER] using /completion fallback, prompt_len={}", prompt.len());

        let req = NativeReq {
            prompt,
            stop: &["<|eot_id|>", "<|eom_id|>", "<|im_end|>"],
            stream: false,
        };

        let raw = self.client
            .post(format!("{}/completion", self.base_url))
            .json(&req)
            .send()
            .await?;

        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("/completion → {} — {}", status, &body[..body.len().min(300)]));
        }

        let resp: NativeResp = raw.json().await?;
        let tps = resp.timings.and_then(|t| t.predicted_per_second).unwrap_or(0.0);
        Ok(ModelResponse { content: resp.content, tokens_per_sec: tps, prompt_tokens: 0, completion_tokens: 0 })
    }
}

/// Format messages using the Llama 3.1 chat template.
/// EXTEND: detect model family and use the appropriate template.
fn format_llama3_prompt(messages: &[Message]) -> String {
    let mut s = String::from("<|begin_of_text|>");
    for m in messages {
        s.push_str(&format!(
            "<|start_header_id|>{}<|end_header_id|>\n\n{}<|eot_id|>",
            m.role, m.content
        ));
    }
    s.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
    s
}

// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ModelAdapter for LlamaCppAdapter {
    async fn complete(&self, context: AssembledContext) -> Result<ModelResponse> {
        let model_id = self.resolve_model_id().await;
        let messages = context.to_messages();
        eprintln!("[ADAPTER] POST /v1/chat/completions with model='{model_id}'");

        // Try OpenAI-compatible endpoint first; fall back to native /completion if 404
        match self.try_v1_chat(&model_id, messages.clone()).await? {
            Some(r) => Ok(r),
            None => {
                eprintln!("[ADAPTER] falling back to native /completion");
                self.native_completion(messages).await
            }
        }
    }

    fn model_id(&self) -> &str {
        &self.fallback_alias
    }
}

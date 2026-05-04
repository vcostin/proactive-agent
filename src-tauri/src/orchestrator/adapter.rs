use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Emitter;
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
    /// Non-streaming completion (fallback).
    async fn complete(&self, context: AssembledContext) -> Result<ModelResponse>;
    /// Streaming completion — emits `chat_token` events as tokens arrive,
    /// emits `chat_done` when finished. Falls back to complete() if not supported.
    async fn complete_streaming(
        &self,
        context: AssembledContext,
        app_handle: &tauri::AppHandle,
    ) -> Result<ModelResponse>;
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
        if !status.is_success() { return None; }

        #[derive(Deserialize)]
        struct ModelsResp { models: Option<Vec<LlamaEntry>>, data: Option<Vec<OaiEntry>> }
        #[derive(Deserialize)]
        struct LlamaEntry { model: String }
        #[derive(Deserialize)]
        struct OaiEntry { id: String }

        serde_json::from_str::<ModelsResp>(&body).ok().and_then(|m| {
            m.models.and_then(|v| v.into_iter().next().map(|e| e.model))
                .or_else(|| m.data.and_then(|v| v.into_iter().next().map(|e| e.id)))
        })
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

    /// Try /v1/chat/completions with streaming=true.
    /// Returns Ok(None) on 404 so caller can fall back.
    async fn stream_v1_chat(
        &self,
        model_id: &str,
        messages: Vec<Message>,
        app_handle: &tauri::AppHandle,
    ) -> Result<Option<ModelResponse>> {
        #[derive(Serialize)]
        struct ChatReq<'a> { model: &'a str, messages: Vec<Message>, stream: bool }
        #[derive(Deserialize)]
        struct StreamEvent { choices: Vec<StreamChoice>, timings: Option<Timings> }
        #[derive(Deserialize)]
        struct StreamChoice { delta: Delta, finish_reason: Option<String> }
        #[derive(Deserialize)]
        struct Delta { content: Option<String> }
        #[derive(Deserialize)]
        struct Timings { predicted_per_second: Option<f64> }

        let req = ChatReq { model: model_id, messages, stream: true };
        let raw = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await?;

        if raw.status().as_u16() == 404 {
            return Ok(None);
        }
        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_default();
            *self.discovered_id.write().await = None;
            return Err(anyhow::anyhow!("model='{}' → {} — {}", model_id, status, &body[..body.len().min(200)]));
        }

        let mut byte_stream = raw.bytes_stream();
        let mut full_content = String::new();
        let mut buffer = String::new();
        let mut tps = 0.0f64;

        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Extract complete SSE lines from buffer
            loop {
                let Some(nl) = buffer.find('\n') else { break };
                let line = buffer[..nl].trim().to_string();
                buffer = buffer[nl + 1..].to_string();

                if !line.starts_with("data:") { continue; }
                let data = line["data:".len()..].trim();
                if data == "[DONE]" { break; }

                if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                    if let Some(t) = event.timings.and_then(|t| t.predicted_per_second) {
                        tps = t;
                    }
                    if let Some(token) = event.choices.first()
                        .and_then(|c| c.delta.content.as_deref())
                    {
                        full_content.push_str(token);
                        let _ = app_handle.emit("chat_token", token);
                    }
                }
            }
        }

        Ok(Some(ModelResponse {
            content: full_content,
            tokens_per_sec: tps,
            prompt_tokens: 0,
            completion_tokens: 0,
        }))
    }

    /// Non-streaming /v1/chat/completions (fallback path).
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

        if raw.status().as_u16() == 404 { return Ok(None); }
        if !raw.status().is_success() {
            let status = raw.status();
            let body = raw.text().await.unwrap_or_default();
            *self.discovered_id.write().await = None;
            return Err(anyhow::anyhow!("model='{}' → {} — {}", model_id, status, &body[..body.len().min(200)]));
        }

        let resp: ChatResp = raw.json().await?;
        let content = resp.choices.into_iter().next().map(|c| c.message.content).unwrap_or_default();
        let tps = resp.timings.and_then(|t| t.predicted_per_second).unwrap_or(0.0);
        let (pt, ct) = resp.usage.map(|u| (u.prompt_tokens, u.completion_tokens)).unwrap_or((0, 0));
        Ok(Some(ModelResponse { content, tokens_per_sec: tps, prompt_tokens: pt, completion_tokens: ct }))
    }

    async fn native_completion(&self, messages: Vec<Message>) -> Result<ModelResponse> {
        #[derive(Serialize)]
        struct NativeReq { prompt: String, stop: Vec<&'static str>, stream: bool }
        #[derive(Deserialize)]
        struct NativeResp { content: String, timings: Option<NativeTimings> }
        #[derive(Deserialize)]
        struct NativeTimings { predicted_per_second: Option<f64> }

        let prompt = format_llama3_prompt(&messages);
        let req = NativeReq {
            prompt,
            stop: vec!["<|eot_id|>", "<|eom_id|>", "<|im_end|>"],
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
            return Err(anyhow::anyhow!("/completion → {} — {}", status, &body[..body.len().min(200)]));
        }
        let resp: NativeResp = raw.json().await?;
        let tps = resp.timings.and_then(|t| t.predicted_per_second).unwrap_or(0.0);
        Ok(ModelResponse { content: resp.content, tokens_per_sec: tps, prompt_tokens: 0, completion_tokens: 0 })
    }
}

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
        match self.try_v1_chat(&model_id, messages.clone()).await? {
            Some(r) => Ok(r),
            None => self.native_completion(messages).await,
        }
    }

    async fn complete_streaming(
        &self,
        context: AssembledContext,
        app_handle: &tauri::AppHandle,
    ) -> Result<ModelResponse> {
        let model_id = self.resolve_model_id().await;
        let messages = context.to_messages();

        // Try streaming first; fall back to non-streaming if unsupported
        match self.stream_v1_chat(&model_id, messages.clone(), app_handle).await? {
            Some(r) => Ok(r),
            None => {
                // Server doesn't support streaming — fall back and emit single token
                let r = self.native_completion(messages).await?;
                let _ = app_handle.emit("chat_token", &r.content);
                Ok(r)
            }
        }
    }

    fn model_id(&self) -> &str { &self.fallback_alias }
}

pub mod adapter;
pub mod context;
pub mod scheduler;

use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::memory::MemoryStore;
use crate::monitor::DeferredMessage;

use adapter::{LlamaCppAdapter, ModelAdapter};
use context::{AssembledContext, Turn};

static DEFER_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Deserialize)]
struct DeferJson {
    message: String,
    after_minutes: i64,
    trigger: String,
}

pub struct Orchestrator {
    pub adapter: Box<dyn ModelAdapter + Send + Sync>,
    pub memory: MemoryStore,
    pub last_context: Option<AssembledContext>,
    recent_turns: Vec<Turn>,
    session_id: String,
    config: Arc<RwLock<AppConfig>>,
}

impl Orchestrator {
    pub async fn new(config: Arc<RwLock<AppConfig>>) -> Result<Self> {
        let (db_path, embed_port, llama_port) = {
            let cfg = config.read().await;
            (
                cfg.db_path.to_str().unwrap_or("data/memory").to_string(),
                cfg.embed_port,
                cfg.llama_port,
            )
        };

        let memory = MemoryStore::open(&db_path, embed_port).await?;
        let adapter: Box<dyn ModelAdapter + Send + Sync> =
            // "llama-chat" matches the --alias flag in start_chat_server
            Box::new(LlamaCppAdapter::new(llama_port, "llama-chat"));

        Ok(Self {
            adapter,
            memory,
            last_context: None,
            recent_turns: Vec::new(),
            session_id: Uuid::new_v4().to_string(),
            config,
        })
    }

    /// Full conversation turn: embed → retrieve → assemble → infer → parse → store.
    /// Streams tokens to the frontend via `chat_token` events on `app_handle`.
    pub async fn send_message(
        &mut self,
        user_input: String,
        app_handle: &tauri::AppHandle,
    ) -> Result<(String, Option<DeferredMessage>)> {
        // Snapshot config — release lock before async LLM call
        let (persona, top_k_ep, top_k_sem, window, temperature, top_p) = {
            let cfg = self.config.read().await;
            (
                cfg.persona_prompt.clone(),
                cfg.top_k_episodic,
                cfg.top_k_semantic,
                cfg.recent_turns_window,
                cfg.temperature,
                cfg.top_p,
            )
        };

        // 1. Embed input — best-effort: if the embed server is down, proceed without memory.
        //    Chat still works; memory retrieval is silently skipped until the server is up.
        let embedding = self.memory.embedding.embed(&user_input).await.ok();

        // 2. Retrieve memories (skipped when embedding unavailable)
        let (episodic, semantic) = if let Some(ref emb) = embedding {
            let ep = self.memory.episodic.retrieve_similar(emb.clone(), top_k_ep).await
                .unwrap_or_default();
            let sem = self.memory.semantic.retrieve_relevant(emb.clone(), top_k_sem).await
                .unwrap_or_default();
            (ep, sem)
        } else {
            (vec![], vec![])
        };

        // 3. Assemble context
        let semantic_facts: Vec<String> = semantic.iter().map(|f| f.fact.clone()).collect();
        let episodic_texts: Vec<String> = episodic.iter().map(|e| e.content.clone()).collect();

        let recent: Vec<Turn> = self
            .recent_turns
            .iter()
            .rev()
            .take(window)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        // 3b. Trim to fit context window — drop oldest episodic entries first
        let mut episodic_texts = episodic_texts;
        let mut recent = recent;
        let limit = {
            let cfg = self.config.read().await;
            cfg.context_window_tokens
        };
        // Gradually remove episodic entries until we fit
        while !episodic_texts.is_empty() {
            let probe = AssembledContext::assemble(&persona, &semantic_facts, &episodic_texts, recent.clone(), user_input.clone());
            if probe.total_tokens() <= limit { break; }
            episodic_texts.remove(0);
        }
        // If still over, trim recent turns from the front
        while recent.len() > 1 {
            let probe = AssembledContext::assemble(&persona, &semantic_facts, &episodic_texts, recent.clone(), user_input.clone());
            if probe.total_tokens() <= limit { break; }
            recent.remove(0);
        }

        let ctx = AssembledContext::assemble(
            &persona,
            &semantic_facts,
            &episodic_texts,
            recent,
            user_input.clone(),
        );

        // 4. Call LLM with streaming — tokens emitted as chat_token events
        let params = crate::orchestrator::adapter::GenParams { temperature, top_p };
        let response = self.adapter.complete_streaming(ctx.clone(), app_handle, params).await?;

        // 5. Parse <defer> tags
        let (clean, deferred) = parse_defer(&response.content);

        // 6. Store both turns — best-effort, skipped when embed server is unavailable
        if let Ok(user_embed) = self.memory.embedding.embed(&user_input).await {
            let importance = importance_heuristic(&user_input);
            let _ = self.memory.episodic.store(
                crate::memory::episodic::RawTurn {
                    session_id: self.session_id.clone(),
                    role: "user".to_string(),
                    content: user_input.clone(),
                    importance_score: importance,
                },
                user_embed,
            ).await;
        }
        if let Ok(asst_embed) = self.memory.embedding.embed(&clean).await {
            let _ = self.memory.episodic.store(
                crate::memory::episodic::RawTurn {
                    session_id: self.session_id.clone(),
                    role: "assistant".to_string(),
                    content: clean.clone(),
                    importance_score: 0.7,
                },
                asst_embed,
            ).await;
        }

        // 7. Update recent-turns window
        self.recent_turns.push(Turn { role: "user".to_string(), content: user_input });
        self.recent_turns.push(Turn { role: "assistant".to_string(), content: clean.clone() });

        // 8. Persist context for debug inspector
        self.last_context = Some(ctx);

        Ok((clean, deferred))
    }

    /// Replace the chat adapter without touching memory or persona.
    /// The alias is always "llama-chat" — matches --alias in start_chat_server.
    pub fn swap_adapter(&mut self, port: u16, _model_path: impl Into<String>) {
        self.adapter = Box::new(LlamaCppAdapter::new(port, "llama-chat"));
    }

    /// Full memory reset: clears episodic + semantic tables and the recent turns window.
    /// Frees the model from past conversation context entirely.
    pub async fn reset_memory(&mut self) -> anyhow::Result<()> {
        // Delete all rows from both LanceDB tables
        self.memory.episodic.clear_all().await?;
        self.memory.semantic.clear_all().await?;
        // Reset the in-memory recent turns window
        self.recent_turns.clear();
        self.last_context = None;
        Ok(())
    }

    /// Run one distillation pass: read recent episodic turns, ask the LLM to
    /// extract durable facts, store them in semantic memory.
    /// Called from a background task — never blocks conversation.
    pub async fn distill(&mut self) -> anyhow::Result<usize> {
        let turns = self.memory.episodic.retrieve_recent(30).await?;
        let count = self.memory.semantic
            .distill_from_episodic(&turns, self.adapter.as_ref(), &self.memory.embedding)
            .await?;
        Ok(count)
    }
}

/// Strip `<defer>` tag and parse its JSON payload.
/// Handles both `<defer>{...}</defer>` (proper) and `<defer>{...}` (no closing tag —
/// models often omit the closing tag). JSON must start with `{`.
fn parse_defer(response: &str) -> (String, Option<DeferredMessage>) {
    let re = DEFER_RE.get_or_init(|| {
        // Match <defer> followed by JSON up to </defer> or end-of-string
        Regex::new(r"(?s)<defer>\s*(\{.*?\})(?:\s*</defer>|$|\n)").expect("static regex is valid")
    });

    if let Some(caps) = re.captures(response) {
        let full_match = caps.get(0).unwrap();
        let json_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");

        // Remove the tag from the response text
        let cleaned = format!(
            "{}{}",
            response[..full_match.start()].trim_end(),
            response[full_match.end()..].trim_start()
        );

        match serde_json::from_str::<DeferJson>(json_str.trim()) {
            Ok(d) => {
                let msg = DeferredMessage {
                    id: Uuid::new_v4().to_string(),
                    message: d.message,
                    trigger: d.trigger,
                    fire_at: Utc::now()
                        + chrono::Duration::minutes(d.after_minutes),
                };
                return (cleaned, Some(msg));
            }
            Err(e) => {
                // Discard malformed defer tag per architecture risk note
                eprintln!("[ORCHESTRATOR] malformed <defer> JSON, discarding: {e}");
                return (cleaned, None);
            }
        }
    }

    (response.to_string(), None)
}

/// Rough importance heuristic — very short turns are low-signal.
/// EXTEND: replace with LLM-scored importance
fn importance_heuristic(text: &str) -> f32 {
    let words = text.split_whitespace().count();
    if words < 5 { 0.3 } else if words > 50 { 0.9 } else { 0.7 }
}

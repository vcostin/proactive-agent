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
        let (db_path, embed_port, llama_port, chat_model) = {
            let cfg = config.read().await;
            (
                cfg.db_path.to_str().unwrap_or("data/memory").to_string(),
                cfg.embed_port,
                cfg.llama_port,
                cfg.chat_model.clone(),
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
    /// Returns the cleaned response text and any parsed deferred message.
    pub async fn send_message(
        &mut self,
        user_input: String,
    ) -> Result<(String, Option<DeferredMessage>)> {
        // Snapshot config — release lock before async LLM call
        let (persona, top_k_ep, top_k_sem, window) = {
            let cfg = self.config.read().await;
            (
                cfg.persona_prompt.clone(),
                cfg.top_k_episodic,
                cfg.top_k_semantic,
                cfg.recent_turns_window,
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

        let ctx = AssembledContext::assemble(
            &persona,
            &semantic_facts,
            &episodic_texts,
            recent,
            user_input.clone(),
        );

        // 4. Call LLM — no locks held here
        let response = self.adapter.complete(ctx.clone()).await?;

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
}

/// Strip `<defer>` tag and parse its JSON payload.
/// Lenient: if JSON is malformed the tag is discarded and logged, not errored.
fn parse_defer(response: &str) -> (String, Option<DeferredMessage>) {
    let re = DEFER_RE.get_or_init(|| {
        Regex::new(r"(?s)<defer>(.*?)</defer>").expect("static regex is valid")
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

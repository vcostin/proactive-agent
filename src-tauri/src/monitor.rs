use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

// ── Sidecar health ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarHealth {
    pub alive: bool,
    pub port: u16,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub last_latency_ms: u64,
    pub last_status_code: u16,
}

// ── Model info ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub filename: String,
    /// Parsed from filename: Q4_K_M, Q5_K_S, etc.
    pub quant_type: String,
    /// Parsed from filename: 7B, 13B, 70B, etc.
    pub param_count: String,
    pub file_size_bytes: u64,
    pub last_modified: DateTime<Utc>,
}

impl ModelInfo {
    /// Best-effort parse of quant and param count from a GGUF filename.
    pub fn parse_filename(filename: &str) -> (String, String) {
        let upper = filename.to_uppercase();

        let quant = ["Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M"]
            .iter()
            .find(|&&q| upper.contains(q))
            .map(|&q| q.to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        let param = ["405B", "236B", "70B", "34B", "13B", "8B", "7B", "3B", "1B"]
            .iter()
            .find(|&&p| upper.contains(p))
            .map(|&p| p.to_string())
            .unwrap_or_else(|| "?B".to_string());

        (quant, param)
    }
}

// ── Memory stats ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub episodic_count: u64,
    pub semantic_count: u64,
    pub last_write: Option<DateTime<Utc>>,
    pub last_distillation: Option<DateTime<Utc>>,
    pub last_embed_latency_ms: u64,
}

// ── Audio state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VadState {
    Silent,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioState {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub vad_state: VadState,
    pub energy_level: f32,
    pub last_stt_result: Option<String>,
    pub last_stt_latency_ms: u64,
    pub last_tts_latency_ms: u64,
    pub tts_buffer_fill_pct: f32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            device_name: "none".to_string(),
            sample_rate: 0,
            channels: 0,
            vad_state: VadState::Silent,
            energy_level: 0.0,
            last_stt_result: None,
            last_stt_latency_ms: 0,
            last_tts_latency_ms: 0,
            tts_buffer_fill_pct: 0.0,
        }
    }
}

// ── Scheduler state ───────────────────────────────────────────────────────────

/// A message the LLM deferred for later delivery.
/// Defined here so both monitor and the scheduler (Phase 3) share the type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredMessage {
    pub id: String,
    pub message: String,
    pub trigger: String,
    pub fire_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerState {
    pub pending: Vec<DeferredMessage>,
    pub last_fired: Option<DeferredMessage>,
}

// ── Top-level status ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub sidecars: HashMap<String, SidecarHealth>,
    pub active_model: Option<ModelInfo>,
    pub memory: MemoryStats,
    pub audio: AudioState,
    pub scheduler: SchedulerState,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self {
            sidecars: HashMap::new(),
            active_model: None,
            memory: MemoryStats {
                episodic_count: 0,
                semantic_count: 0,
                last_write: None,
                last_distillation: None,
                last_embed_latency_ms: 0,
            },
            audio: AudioState::default(),
            scheduler: SchedulerState { pending: vec![], last_fired: None },
        }
    }
}

// ── Debug event log ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugEvent {
    pub timestamp: DateTime<Utc>,
    /// Component tag: [MEMORY], [AUDIO], [SCHEDULER], [ADAPTER], [ORCHESTRATOR]
    pub component: String,
    pub message: String,
}

/// Shared ring buffer for live debug events. Max 500 entries.
/// Uses std::sync::Mutex so it's safe to write from both sync callbacks and async tasks.
pub type SharedEventLog = Arc<Mutex<VecDeque<DebugEvent>>>;

pub const MAX_EVENT_LOG: usize = 500;

pub fn new_event_log() -> SharedEventLog {
    Arc::new(Mutex::new(VecDeque::with_capacity(MAX_EVENT_LOG)))
}

/// Append a debug event. Non-blocking — drops the write if the lock is poisoned.
pub fn push_event(log: &SharedEventLog, component: &str, message: impl Into<String>) {
    if let Ok(mut guard) = log.lock() {
        if guard.len() >= MAX_EVENT_LOG {
            guard.pop_front();
        }
        guard.push_back(DebugEvent {
            timestamp: Utc::now(),
            component: component.to_string(),
            message: message.into(),
        });
    }
}

/// Append and also emit a live `debug_event` Tauri event so the frontend
/// event log updates without waiting for the next poll.
pub async fn emit_debug_event(
    app_handle: &tauri::AppHandle,
    log: &SharedEventLog,
    component: &str,
    message: impl Into<String>,
) {
    let event = DebugEvent {
        timestamp: Utc::now(),
        component: component.to_string(),
        message: message.into(),
    };
    if let Ok(mut guard) = log.lock() {
        if guard.len() >= MAX_EVENT_LOG {
            guard.pop_front();
        }
        guard.push_back(event.clone());
    }
    let _ = app_handle.emit("debug_event", &event);
}

// ── Health-check polling loop ─────────────────────────────────────────────────

/// Long-running task — polls each sidecar's /health endpoint every 5 seconds
/// and emits `sidecar_health` events to the frontend.
pub async fn run_monitor_loop(
    config: Arc<tokio::sync::RwLock<crate::config::AppConfig>>,
    event_log: SharedEventLog,
    app_handle: tauri::AppHandle,
) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    // Track previous alive state per sidecar — only log on state transitions
    let mut prev_alive: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        interval.tick().await;
        let (llama_port, embed_port, stt_port) = {
            let cfg = config.read().await;
            (cfg.llama_port, cfg.embed_port, cfg.stt_port)
        };

        let sidecars = [
            ("llama",    llama_port),
            ("embed",    embed_port),
            ("parakeet", stt_port),
        ];

        for (name, port) in sidecars {
            let url = format!("http://127.0.0.1:{port}/health");
            let start = std::time::Instant::now();
            let (alive, status_code) =
                match client.get(&url).send().await {
                    Ok(r) => (r.status().is_success(), r.status().as_u16()),
                    Err(_) => (false, 0u16),
                };
            let latency_ms = start.elapsed().as_millis() as u64;

            // Only emit event log entry when state changes — not every 5 seconds
            let was_alive = prev_alive.get(name).copied();
            if was_alive != Some(alive) {
                let msg = if alive {
                    format!("sidecar {name} :{port} → online ({latency_ms}ms)")
                } else {
                    format!("sidecar {name} :{port} → offline")
                };
                emit_debug_event(&app_handle, &event_log, "[MONITOR]", msg).await;
                prev_alive.insert(name, alive);
            }

            let _ = app_handle.emit(
                "sidecar_health",
                serde_json::json!({
                    "name": name,
                    "alive": alive,
                    "port": port,
                    "latency_ms": latency_ms,
                    "status_code": status_code,
                }),
            );
        }
    }
}

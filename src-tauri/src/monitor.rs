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

/// Tri-state for a sidecar: online, loading model, or offline.
/// "loading" is distinct from "offline" — it means the process is up
/// but not yet ready (llama-server returns 503 while loading the GGUF).
#[derive(Debug, Clone, PartialEq)]
enum SidecarState {
    Online,
    Loading,  // 503 + body contains "loading model"
    Offline,
}

impl SidecarState {
    fn is_alive(&self) -> bool { matches!(self, SidecarState::Online) }
    fn as_str(&self) -> &'static str {
        match self {
            SidecarState::Online  => "online",
            SidecarState::Loading => "loading",
            SidecarState::Offline => "offline",
        }
    }
}

/// Poll a single sidecar and return its state + http status code.
async fn poll_sidecar(client: &Client, port: u16) -> (SidecarState, u16, u64) {
    let url = format!("http://{}:{port}/health", crate::SIDECAR_HOST);
    let start = std::time::Instant::now();
    match client.get(&url).send().await {
        Ok(r) => {
            let status = r.status();
            let latency = start.elapsed().as_millis() as u64;
            let state = if status.is_success() {
                SidecarState::Online
            } else if status.as_u16() == 503 {
                // llama.cpp returns 503 + {"status":"loading model"} while the GGUF loads
                let body = r.text().await.unwrap_or_default();
                if body.contains("loading") {
                    SidecarState::Loading
                } else {
                    SidecarState::Offline
                }
            } else {
                SidecarState::Offline
            };
            (state, status.as_u16(), latency)
        }
        Err(_) => (SidecarState::Offline, 0, start.elapsed().as_millis() as u64),
    }
}

/// Long-running task — polls each sidecar's /health endpoint every 5 seconds
/// and emits `sidecar_health` events to the frontend.
///
/// State transitions are debounced: a state change is only committed (and logged)
/// after 2 consecutive polls agree. This eliminates the online/offline flicker
/// that occurs while llama-server is loading its model file.
pub async fn run_monitor_loop(
    config: Arc<tokio::sync::RwLock<crate::config::AppConfig>>,
    event_log: SharedEventLog,
    app_handle: tauri::AppHandle,
) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    // committed: last stable state per sidecar
    // pending:   (candidate state, consecutive count) — must reach 2 to commit
    let mut committed: std::collections::HashMap<String, SidecarState> = std::collections::HashMap::new();
    let mut pending:   std::collections::HashMap<String, (SidecarState, u8)> = std::collections::HashMap::new();

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        interval.tick().await;
        let (llama_port, embed_port) = {
            let cfg = config.read().await;
            (cfg.llama_port, cfg.embed_port)
        };

        // Parakeet removed — STT now runs in-process via ort, no HTTP port
        let sidecars = [
            ("llama", llama_port),
            ("embed", embed_port),
        ];

        for (name, port) in sidecars {
            let (state, status_code, latency_ms) = poll_sidecar(&client, port).await;

            // Debounce: accumulate consecutive polls, commit after 2 agree
            let confirmed = {
                let entry = pending.entry(name.to_string()).or_insert((state.clone(), 0));
                if entry.0 == state {
                    entry.1 += 1;
                } else {
                    *entry = (state.clone(), 1);
                }
                entry.1 >= 2
            };

            let prev = committed.get(name);
            if confirmed && prev != Some(&state) {
                let msg = match &state {
                    SidecarState::Online  => format!("sidecar {name} :{port} → online ({latency_ms}ms)"),
                    SidecarState::Loading => format!("sidecar {name} :{port} → loading model…"),
                    SidecarState::Offline => format!("sidecar {name} :{port} → offline"),
                };
                emit_debug_event(&app_handle, &event_log, "[MONITOR]", msg).await;
                committed.insert(name.to_string(), state.clone());
            }

            let _ = app_handle.emit(
                "sidecar_health",
                serde_json::json!({
                    "name":        name,
                    "alive":       state.is_alive(),
                    "state":       state.as_str(),   // "online" | "loading" | "offline"
                    "port":        port,
                    "latency_ms":  latency_ms,
                    "status_code": status_code,
                }),
            );
        }
    }
}

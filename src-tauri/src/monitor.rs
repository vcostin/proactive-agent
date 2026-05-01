use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

// EXTEND: add health-check polling loop and debug_event emitter (Phase 4)

use std::collections::HashMap;

use chrono::Utc;
use tauri::{Emitter, State};

use crate::monitor::{AudioState, MemoryStats, ModelInfo, SystemStatus};
use crate::orchestrator::context::AssembledContext;
use crate::{SharedConfig, SharedOrchestrator, SharedScheduler};

type CmdResult<T> = Result<T, String>;

fn to_cmd_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Chat ──────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn send_message(
    orchestrator: State<'_, SharedOrchestrator>,
    scheduler: State<'_, SharedScheduler>,
    app_handle: tauri::AppHandle,
    message: String,
) -> CmdResult<String> {
    let mut lock = orchestrator.lock().await;
    let orch = lock.as_mut().ok_or("Orchestrator not yet initialised")?;

    let (response, deferred) =
        orch.send_message(message).await.map_err(to_cmd_err)?;

    if let Some(msg) = deferred {
        scheduler.lock().await.add(msg.clone());
        // Emit immediately so the debug scheduler panel updates without waiting
        let _ = app_handle.emit("scheduler_updated", ());
    }

    Ok(response)
}

/// Hot-swap the loaded chat model without restarting.
/// Does not touch the embedding model.
#[tauri::command]
pub async fn swap_model(
    config: State<'_, SharedConfig>,
    orchestrator: State<'_, SharedOrchestrator>,
    model_filename: String,
) -> CmdResult<()> {
    let port = config.read().await.llama_port;
    let mut lock = orchestrator.lock().await;
    if let Some(ref mut orch) = *lock {
        orch.swap_adapter(port, &model_filename);
        config.write().await.chat_model = model_filename;
    }
    Ok(())
}

// ── Memory ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_memories(
    orchestrator: State<'_, SharedOrchestrator>,
    query: String,
) -> CmdResult<Vec<String>> {
    let mut lock = orchestrator.lock().await;
    let orch = lock.as_mut().ok_or("Orchestrator not yet initialised")?;

    let embedding = orch.memory.embedding.embed(&query).await.map_err(to_cmd_err)?;
    let episodic = orch
        .memory
        .episodic
        .retrieve_similar(embedding.clone(), 5)
        .await
        .map_err(to_cmd_err)?;
    let semantic = orch
        .memory
        .semantic
        .retrieve_relevant(embedding, 5)
        .await
        .map_err(to_cmd_err)?;

    let mut results: Vec<String> = episodic.iter().map(|e| e.content.clone()).collect();
    results.extend(semantic.iter().map(|f| f.fact.clone()));
    Ok(results)
}

// ── Debug / monitoring ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_system_status(
    orchestrator: State<'_, SharedOrchestrator>,
    scheduler: State<'_, SharedScheduler>,
) -> CmdResult<SystemStatus> {
    let orch_lock = orchestrator.lock().await;
    let sched_lock = scheduler.lock().await;

    let active_model = orch_lock.as_ref().map(|o| {
        let id = o.adapter.model_id();
        let (quant_type, param_count) = ModelInfo::parse_filename(id);
        ModelInfo {
            filename: id.to_string(),
            quant_type,
            param_count,
            file_size_bytes: 0,
            last_modified: Utc::now(),
        }
    });

    let embed_latency = orch_lock
        .as_ref()
        .map(|o| o.memory.embedding.last_latency_ms())
        .unwrap_or(0);

    Ok(SystemStatus {
        sidecars: HashMap::new(), // EXTEND: Phase 4 — health-check polling
        active_model,
        memory: MemoryStats {
            episodic_count: 0,    // EXTEND: Phase 4 — query table row count
            semantic_count: 0,
            last_write: None,
            last_distillation: None,
            last_embed_latency_ms: embed_latency,
        },
        audio: AudioState::default(), // EXTEND: Phase 4
        scheduler: sched_lock.state(),
    })
}

#[tauri::command]
pub async fn get_last_context(
    orchestrator: State<'_, SharedOrchestrator>,
) -> CmdResult<Option<AssembledContext>> {
    Ok(orchestrator.lock().await.as_ref().and_then(|o| o.last_context.clone()))
}

#[tauri::command]
pub async fn fire_deferred_now(
    scheduler: State<'_, SharedScheduler>,
    app_handle: tauri::AppHandle,
    id: String,
) -> CmdResult<()> {
    let mut sched = scheduler.lock().await;
    match sched.fire_now(&id) {
        Some(msg) => {
            app_handle.emit("proactive_message", &msg).map_err(to_cmd_err)?;
            Ok(())
        }
        None => Err(format!("no pending message with id {id}")),
    }
}

// ── Model management ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_models(config: State<'_, SharedConfig>) -> CmdResult<Vec<ModelInfo>> {
    let models_dir = config.read().await.models_dir.clone();

    if !models_dir.exists() {
        return Ok(vec![]);
    }

    let mut models = Vec::new();
    let entries =
        std::fs::read_dir(&models_dir).map_err(|e| format!("cannot read models dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let meta = entry.metadata().map_err(to_cmd_err)?;
        let file_size_bytes = meta.len();
        let last_modified = meta
            .modified()
            .map(|t| {
                let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default()
            })
            .unwrap_or_default();

        let (quant_type, param_count) = ModelInfo::parse_filename(&filename);
        models.push(ModelInfo { filename, quant_type, param_count, file_size_bytes, last_modified });
    }

    models.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(models)
}

// ── Event log ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_debug_events(
    limit: Option<usize>,
) -> CmdResult<Vec<serde_json::Value>> {
    // EXTEND: Phase 4 — return last N events from shared ring-buffer in monitor
    let _ = limit;
    Ok(vec![])
}

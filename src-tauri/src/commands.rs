use crate::monitor::{ModelInfo, SystemStatus};
use crate::orchestrator::context::AssembledContext;
use crate::SharedConfig;
use tauri::State;

type CmdResult<T> = Result<T, String>;

fn to_cmd_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Chat ──────────────────────────────────────────────────────────────────────

/// Main conversation entry point. Phase 3 wires this to the full orchestrator.
#[tauri::command]
pub async fn send_message(
    _config: State<'_, SharedConfig>,
    message: String,
) -> CmdResult<String> {
    // EXTEND: Phase 3 — assemble context, call adapter, parse <defer> tags, store turn
    let _ = message;
    Err("orchestrator not yet wired (Phase 3)".to_string())
}

/// Hot-swap the loaded chat model. Does not affect the embedding model.
#[tauri::command]
pub async fn swap_model(
    config: State<'_, SharedConfig>,
    model_filename: String,
) -> CmdResult<()> {
    // EXTEND: Phase 3 — signal llama-server to reload the model
    let mut cfg = config.write().await;
    cfg.chat_model = model_filename;
    Ok(())
}

// ── Memory ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_memories(
    _config: State<'_, SharedConfig>,
    query: String,
) -> CmdResult<Vec<String>> {
    // EXTEND: Phase 2 — query EpisodicStore and SemanticStore
    let _ = query;
    Err("memory store not yet wired (Phase 2)".to_string())
}

// ── Debug / monitoring ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_system_status(
    _config: State<'_, SharedConfig>,
) -> CmdResult<SystemStatus> {
    // EXTEND: Phase 4 — read from shared monitor state updated by polling loop
    Ok(SystemStatus::default())
}

#[tauri::command]
pub async fn get_last_context(
    _config: State<'_, SharedConfig>,
) -> CmdResult<Option<AssembledContext>> {
    // EXTEND: Phase 3 — return last AssembledContext stored by the orchestrator
    Ok(None)
}

#[tauri::command]
pub async fn fire_deferred_now(
    _config: State<'_, SharedConfig>,
    id: String,
) -> CmdResult<()> {
    // EXTEND: Phase 3 — look up DeferredMessage by id in scheduler, fire immediately
    let _ = id;
    Err("scheduler not yet wired (Phase 3)".to_string())
}

// ── Model management ──────────────────────────────────────────────────────────

/// Scan the models directory and return metadata for every *.gguf file found.
#[tauri::command]
pub async fn list_models(config: State<'_, SharedConfig>) -> CmdResult<Vec<ModelInfo>> {
    let models_dir = {
        let cfg = config.read().await;
        cfg.models_dir.clone()
    };

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
                let duration = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                    .unwrap_or_default()
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
    _config: State<'_, SharedConfig>,
    limit: Option<usize>,
) -> CmdResult<Vec<serde_json::Value>> {
    // EXTEND: Phase 4 — return last N events from shared ring-buffer in monitor
    let _ = limit;
    Ok(vec![])
}

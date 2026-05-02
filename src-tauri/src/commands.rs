use std::collections::HashMap;

use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::monitor::{AudioState, MemoryStats, ModelInfo, SystemStatus};
use crate::orchestrator::context::AssembledContext;
use crate::monitor::SharedEventLog;
use crate::{SharedChatChild, SharedConfig, SharedOrchestrator, SharedScheduler};

type CmdResult<T> = Result<T, String>;

fn to_cmd_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Setup / first-run ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupStatus {
    pub ready: bool,
    pub chat_model: String,
    pub embed_model_ready: bool,
    pub whisper_model_ready: bool,
    pub data_dir: String,
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub filename: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
}

/// Returns current setup state — drives the first-run wizard.
#[tauri::command]
pub async fn get_setup_status(config: State<'_, SharedConfig>) -> CmdResult<SetupStatus> {
    let cfg = config.read().await;
    Ok(SetupStatus {
        ready: cfg.is_ready(),
        chat_model: cfg.chat_model.clone(),
        embed_model_ready: cfg.embed_model_path().exists(),
        whisper_model_ready: cfg.whisper_model_path().exists(),
        data_dir: cfg.models_dir
            .parent()
            .unwrap_or(&cfg.models_dir)
            .to_string_lossy()
            .into_owned(),
    })
}

/// Open a native file-picker filtered to .gguf files.
/// Returns the absolute path the user chose, or null if they cancelled.
#[tauri::command]
pub async fn pick_model_file(app_handle: tauri::AppHandle) -> CmdResult<Option<String>> {
    // The blocking dialog call must run outside the async executor
    let path = tokio::task::spawn_blocking(move || {
        app_handle
            .dialog()
            .file()
            .add_filter("GGUF Model", &["gguf"])
            .blocking_pick_file()
    })
    .await
    .map_err(to_cmd_err)?;

    Ok(path.map(|p| p.to_string()))
}

/// Download the two required fixed models (nomic-embed-text + whisper-base-en)
/// into the app data models directory. Emits `download_progress` events.
#[tauri::command]
pub async fn download_required_models(
    config: State<'_, SharedConfig>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let (models_dir, embed_path, whisper_path) = {
        let cfg = config.read().await;
        (cfg.models_dir.clone(), cfg.embed_model_path(), cfg.whisper_model_path())
    };

    std::fs::create_dir_all(&models_dir).map_err(to_cmd_err)?;

    let downloads: &[(&str, &str, &std::path::Path)] = &[
        (
            "nomic-embed-text-v1.5.Q8_0.gguf",
            "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf",
            &embed_path,
        ),
        (
            "ggml-base.en.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
            &whisper_path,
        ),
    ];

    let client = reqwest::Client::new();

    for (filename, url, dest) in downloads {
        if dest.exists() {
            let _ = app_handle.emit("download_progress", DownloadProgress {
                filename: filename.to_string(),
                downloaded: dest.metadata().map(|m| m.len()).unwrap_or(0),
                total: dest.metadata().map(|m| m.len()).unwrap_or(0),
                done: true,
            });
            continue;
        }

        let resp = client.get(*url).send().await.map_err(to_cmd_err)?;
        let total = resp.content_length().unwrap_or(0);
        let mut downloaded = 0u64;

        let mut stream = resp.bytes_stream();
        let mut file = tokio::fs::File::create(dest).await.map_err(to_cmd_err)?;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(to_cmd_err)?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.map_err(to_cmd_err)?;
            downloaded += chunk.len() as u64;
            let _ = app_handle.emit("download_progress", DownloadProgress {
                filename: filename.to_string(),
                downloaded,
                total,
                done: false,
            });
        }

        let _ = app_handle.emit("download_progress", DownloadProgress {
            filename: filename.to_string(),
            downloaded,
            total,
            done: true,
        });
    }

    Ok(())
}

// ── Dependency checks ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SystemDeps {
    pub vcredist_ok: bool,
    pub vulkan_ok: bool,
    /// Result of actually running llama-server --version
    pub llama_server_ok: bool,
    pub llama_server_msg: String,
}

/// Check system-level dependencies required by the sidecar binaries.
#[tauri::command]
pub async fn check_system_deps() -> CmdResult<SystemDeps> {
    #[cfg(target_os = "windows")]
    {
        // VCRUNTIME140_1.dll ships with Visual C++ 2019/2022 runtime
        let vcredist_ok =
            std::path::Path::new("C:\\Windows\\System32\\VCRUNTIME140_1.dll").exists();
        let vulkan_ok =
            std::path::Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists();

        let (llama_server_ok, llama_server_msg) = test_llama_binary().await;
        return Ok(SystemDeps { vcredist_ok, vulkan_ok, llama_server_ok, llama_server_msg });
    }
    #[cfg(not(target_os = "windows"))]
    Ok(SystemDeps {
        vcredist_ok: true,
        vulkan_ok: true,
        llama_server_ok: false,
        llama_server_msg: "not checked on this platform".to_string(),
    })
}

/// Run `llama-server --version` to verify DLLs and entry points resolve correctly.
async fn test_llama_binary() -> (bool, String) {
    let bin_dir = crate::binaries_dir();
    let binary = bin_dir.join(crate::sidecar_filename("llama-server"));

    if !binary.exists() || binary.metadata().map(|m| m.len()).unwrap_or(0) < 1024 {
        return (false, "binary not found — run npm run setup".to_string());
    }

    let priority_path = format!(
        "{};{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(&binary)
            .arg("--version")
            .current_dir(&bin_dir)
            .env("PATH", &priority_path)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) => {
            let code = out.status.code().unwrap_or(-1);
            if out.status.success() || code == 0 {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let text = if text.is_empty() {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                } else { text };
                (true, format!("OK — {}", text.lines().next().unwrap_or("ready")))
            } else {
                let desc = match code as u32 {
                    0xC0000135 => "DLL not found — re-run: npm run setup".to_string(),
                    0xC0000139 => "DLL version mismatch — install Visual C++ Redistributable".to_string(),
                    0xC0000005 => "crash (access violation)".to_string(),
                    _ => format!("exited with code {code:#X}"),
                };
                (false, desc)
            }
        }
        Ok(Err(e)) => (false, format!("failed to launch: {e}")),
        Err(_) => (true, "OK — timed out waiting for --version (normal for some builds)".to_string()),
    }
}

/// Download and silently install the Visual C++ Redistributable 2022 x64.
/// Emits `download_progress` events during download.
#[tauri::command]
pub async fn install_vcredist(app_handle: tauri::AppHandle) -> CmdResult<()> {
    let url = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
    let dest = std::env::temp_dir().join("vc_redist.x64.exe");

    // Download
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await.map_err(to_cmd_err)?;
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut file = tokio::fs::File::create(&dest).await.map_err(to_cmd_err)?;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(to_cmd_err)?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.map_err(to_cmd_err)?;
        downloaded += chunk.len() as u64;
        let _ = app_handle.emit("download_progress", DownloadProgress {
            filename: "vc_redist.x64.exe".to_string(),
            downloaded,
            total,
            done: false,
        });
    }
    drop(file);

    // Silent install
    let status = tokio::process::Command::new(&dest)
        .args(["/install", "/quiet", "/norestart"])
        .status()
        .await
        .map_err(to_cmd_err)?;

    let _ = app_handle.emit("download_progress", DownloadProgress {
        filename: "vc_redist.x64.exe".to_string(),
        downloaded,
        total,
        done: true,
    });

    // Exit code 0 = success, 3010 = success + reboot suggested (not required)
    match status.code() {
        Some(0) | Some(3010) => Ok(()),
        code => Err(format!("installer exited with code {code:?}")),
    }
}

// ── Model clear ───────────────────────────────────────────────────────────────

/// Unload the current chat model: kills the server, clears config, returns to setup wizard.
#[tauri::command]
pub async fn clear_model(
    config: State<'_, SharedConfig>,
    chat_child: State<'_, SharedChatChild>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    // Kill the running chat server
    {
        let mut guard = chat_child.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = child.kill().await;
        }
    }
    // Clear from persisted config
    {
        let mut cfg = config.write().await;
        cfg.chat_model = String::new();
        if let Ok(config_path) = app_handle.path().app_config_dir() {
            let _ = cfg.save(&config_path.join("config.json"));
        }
    }
    Ok(())
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
/// `model_path` is the absolute path to the .gguf file.
/// Kills any running chat llama-server and starts a new one.
#[tauri::command]
pub async fn swap_model(
    config: State<'_, SharedConfig>,
    orchestrator: State<'_, SharedOrchestrator>,
    event_log: State<'_, SharedEventLog>,
    chat_child: State<'_, SharedChatChild>,
    app_handle: tauri::AppHandle,
    model_path: String,
) -> CmdResult<()> {
    let port = {
        let mut cfg = config.write().await;
        cfg.chat_model = model_path.clone();
        if let Ok(config_path) = app_handle.path().app_config_dir() {
            let _ = cfg.save(&config_path.join("config.json"));
        }
        cfg.llama_port
    };

    // Update the in-memory adapter (points to the same port, new model id)
    {
        let mut lock = orchestrator.lock().await;
        if let Some(ref mut orch) = *lock {
            orch.swap_adapter(port, &model_path);
        }
    }

    // Start (or restart) the llama-server process with the new model
    crate::start_chat_server(
        model_path,
        port,
        event_log.inner().clone(),
        chat_child.inner().clone(),
    );

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

    let (episodic_count, semantic_count) = match orch_lock.as_ref() {
        Some(o) => (o.memory.episodic_count().await, o.memory.semantic_count().await),
        None => (0, 0),
    };

    Ok(SystemStatus {
        sidecars: HashMap::new(), // populated live via sidecar_health Tauri events
        active_model,
        memory: MemoryStats {
            episodic_count,
            semantic_count,
            last_write: None,       // EXTEND: track in EpisodicStore
            last_distillation: None,
            last_embed_latency_ms: embed_latency,
        },
        audio: AudioState::default(), // EXTEND: read from SharedAudioState once audio started
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
    let (models_dir, active_model) = {
        let cfg = config.read().await;
        (cfg.models_dir.clone(), cfg.chat_model.clone())
    };

    // Always include the currently active model even if it's outside models_dir
    let mut extra: Vec<ModelInfo> = vec![];
    if !active_model.is_empty() {
        let p = std::path::Path::new(&active_model);
        if p.exists() && p.parent() != Some(&models_dir) {
            let meta = std::fs::metadata(p).ok();
            let (quant, param) = ModelInfo::parse_filename(
                p.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            );
            extra.push(ModelInfo {
                filename: active_model.clone(),
                quant_type: quant,
                param_count: param,
                file_size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                last_modified: meta
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        let secs = t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_secs();
                        chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default()
                    })
                    .unwrap_or_default(),
            });
        }
    }

    let models_dir = models_dir;

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
    models.extend(extra);
    Ok(models)
}

// ── Event log ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_debug_events(
    event_log: State<'_, SharedEventLog>,
    limit: Option<usize>,
) -> CmdResult<Vec<serde_json::Value>> {
    let n = limit.unwrap_or(100).min(500);
    let guard = event_log.lock().map_err(to_cmd_err)?;
    let events = guard
        .iter()
        .rev()
        .take(n)
        .map(|e| {
            serde_json::json!({
                "timestamp": e.timestamp.to_rfc3339(),
                "component": e.component,
                "message": e.message,
            })
        })
        .collect();
    Ok(events)
}

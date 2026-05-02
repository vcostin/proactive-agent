mod audio;
mod commands;
mod config;
mod memory;
mod monitor;
mod orchestrator;

use config::AppConfig;
use monitor::{new_event_log, run_monitor_loop, SharedEventLog};
use orchestrator::{scheduler::ProactivityScheduler, Orchestrator};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex, RwLock};

pub type SharedConfig = Arc<RwLock<AppConfig>>;
pub type SharedOrchestrator = Arc<Mutex<Option<Orchestrator>>>;
pub type SharedScheduler = Arc<Mutex<ProactivityScheduler>>;
/// Holds the chat llama-server child process so it can be killed on model swap.
pub type SharedChatChild = Arc<Mutex<Option<tokio::process::Child>>>;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let data_dir = app.path().app_data_dir().expect("cannot resolve app data dir");
            let config_path = app.path()
                .app_config_dir()
                .expect("cannot resolve app config dir")
                .join("config.json");

            std::fs::create_dir_all(&data_dir).ok();
            let cfg = AppConfig::load(&config_path, data_dir);
            std::fs::create_dir_all(&cfg.models_dir).ok();
            std::fs::create_dir_all(&cfg.db_path).ok();

            let config: SharedConfig = Arc::new(RwLock::new(cfg));
            let orchestrator: SharedOrchestrator = Arc::new(Mutex::new(None));
            let scheduler: SharedScheduler = Arc::new(Mutex::new(ProactivityScheduler::new()));
            let event_log: SharedEventLog = new_event_log();
            let chat_child: SharedChatChild = Arc::new(Mutex::new(None));

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());
            app.manage(event_log.clone());
            app.manage(chat_child.clone());

            // ── Sidecar processes ─────────────────────────────────────────────
            spawn_sidecars(config.clone(), event_log.clone(), chat_child.clone());

            // ── Orchestrator async init ───────────────────────────────────────
            let orch_init = orchestrator.clone();
            let cfg_init = config.clone();
            let handle_init = app_handle.clone();
            let log_init = event_log.clone();
            tauri::async_runtime::spawn(async move {
                monitor::emit_debug_event(&handle_init, &log_init, "[ORCHESTRATOR]", "initialising…").await;
                match Orchestrator::new(cfg_init).await {
                    Ok(o) => {
                        *orch_init.lock().await = Some(o);
                        monitor::emit_debug_event(&handle_init, &log_init, "[ORCHESTRATOR]", "ready").await;
                        let _ = handle_init.emit("orchestrator_ready", ());
                    }
                    Err(e) => {
                        let msg = format!("init failed: {e}");
                        monitor::emit_debug_event(&handle_init, &log_init, "[ORCHESTRATOR]", &msg).await;
                        let _ = handle_init.emit("init_error", e.to_string());
                    }
                }
            });

            // ── Proactivity scheduler loop ────────────────────────────────────
            let sched_loop = scheduler.clone();
            let handle_sched = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                orchestrator::scheduler::run_scheduler_loop(sched_loop, handle_sched).await;
            });

            // ── Sidecar health monitor loop ───────────────────────────────────
            let cfg_monitor = config.clone();
            let log_monitor = event_log.clone();
            let handle_monitor = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                run_monitor_loop(cfg_monitor, log_monitor, handle_monitor).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_setup_status,
            commands::pick_model_file,
            commands::download_required_models,
            commands::send_message,
            commands::swap_model,
            commands::get_memories,
            commands::get_system_status,
            commands::get_last_context,
            commands::fire_deferred_now,
            commands::list_models,
            commands::get_debug_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Process helpers ───────────────────────────────────────────────────────────

/// Resolves the `binaries/` directory regardless of where CWD is.
/// In dev mode CWD is src-tauri/; in release the exe sits next to the binaries.
pub fn binaries_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    // Dev mode: CWD == …/src-tauri/ — step up to project root
    let root = match cwd.file_name().and_then(|n| n.to_str()) {
        Some("src-tauri") => cwd.parent().unwrap_or(&cwd).to_path_buf(),
        _ => cwd,
    };
    root.join("binaries")
}

/// Platform-specific binary filename with Tauri target-triple suffix.
pub fn sidecar_filename(name: &str) -> String {
    #[cfg(target_os = "windows")]
    return format!("{name}-x86_64-pc-windows-msvc.exe");
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") {
            return format!("{name}-aarch64-apple-darwin");
        }
        return format!("{name}-x86_64-apple-darwin");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return format!("{name}-x86_64-unknown-linux-gnu");
}

/// Start (or restart) the chat llama-server.
/// Sets current_dir to binaries/ so Windows finds all bundled DLLs.
pub fn start_chat_server(
    model_path: String,
    port: u16,
    event_log: SharedEventLog,
    chat_child: SharedChatChild,
) {
    tauri::async_runtime::spawn(async move {
        // Kill previous instance and wait for port to free
        {
            let mut guard = chat_child.lock().await;
            if let Some(mut old) = guard.take() {
                let _ = old.kill().await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        let bin_dir = binaries_dir();
        let binary = bin_dir.join(sidecar_filename("llama-server"));

        if !binary.exists() || binary.metadata().map(|m| m.len()).unwrap_or(0) < 1024 {
            monitor::push_event(&event_log, "[ADAPTER]",
                format!("llama (chat): binary not found at {}", binary.display()));
            return;
        }

        monitor::push_event(&event_log, "[ADAPTER]",
            format!("llama (chat) launching → {}", binary.display()));

        let spawn_result = tokio::process::Command::new(&binary)
            .args(["--model", &model_path,
                   "--port", &port.to_string(),
                   "--host", "127.0.0.1",
                   "--ctx-size", "4096",
                   "-ngl", "999"])
            .current_dir(&bin_dir)          // DLLs live here — critical on Windows
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match spawn_result {
            Ok(mut child) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("llama (chat) started (pid {:?})", child.id()));

                let stderr = child.stderr.take();
                { *chat_child.lock().await = Some(child); }

                stream_stderr(stderr, "llama (chat)", event_log.clone()).await;

                // Reap exit code once stderr closes
                let mut guard = chat_child.lock().await;
                if let Some(mut c) = guard.take() {
                    if let Ok(s) = c.wait().await {
                        monitor::push_event(&event_log, "[ADAPTER]",
                            format!("llama (chat) exited (code {:?})", s.code()));
                    }
                }
            }
            Err(e) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("llama (chat) failed to start: {e}"));
            }
        }
    });
}

fn spawn_sidecars(
    config: SharedConfig,
    event_log: SharedEventLog,
    chat_child: SharedChatChild,
) {
    tauri::async_runtime::spawn(async move {
        let cfg = config.read().await;
        let chat_model   = cfg.chat_model.clone();
        let embed_path   = cfg.embed_model_path().to_string_lossy().into_owned();
        let whisper_path = cfg.whisper_model_path().to_string_lossy().into_owned();
        let embed_port   = cfg.embed_port;
        let whisper_port = cfg.whisper_port;
        let kokoro_port  = cfg.kokoro_port;
        let llama_port   = cfg.llama_port;
        drop(cfg);

        if chat_model.is_empty() {
            monitor::push_event(&event_log, "[ADAPTER]",
                "chat model not configured — pick one in the Models tab");
        } else {
            start_chat_server(chat_model, llama_port, event_log.clone(), chat_child);
        }

        spawn_direct("llama-server", "llama (embed)",
            vec!["--model".into(), embed_path,
                 "--port".into(), embed_port.to_string(),
                 "--host".into(), "127.0.0.1".into(),
                 "--ctx-size".into(), "512".into(),
                 "-ngl".into(), "999".into(),
                 "--embedding".into()],
            event_log.clone());

        spawn_direct("whisper-server", "whisper",
            vec!["-m".into(), whisper_path,
                 "--port".into(), whisper_port.to_string(),
                 "--host".into(), "127.0.0.1".into()],
            event_log.clone());

        spawn_direct("kokoro-server", "kokoro",
            vec!["--port".into(), kokoro_port.to_string(),
                 "--host".into(), "127.0.0.1".into()],
            event_log.clone());
    });
}

/// Spawn a sidecar using tokio::process::Command with current_dir = binaries/.
/// This ensures Windows DLL search starts in the right directory.
fn spawn_direct(
    binary_name: &'static str,
    display_name: &'static str,
    args: Vec<String>,
    event_log: SharedEventLog,
) {
    let bin_dir = binaries_dir();
    let binary = bin_dir.join(sidecar_filename(binary_name));

    if !binary.exists() || binary.metadata().map(|m| m.len()).unwrap_or(0) < 1024 {
        monitor::push_event(&event_log, "[ADAPTER]",
            format!("{display_name}: binary not found or placeholder — skipping"));
        return;
    }

    tauri::async_runtime::spawn(async move {
        monitor::push_event(&event_log, "[ADAPTER]",
            format!("{display_name} launching → {}", binary.display()));

        let spawn_result = tokio::process::Command::new(&binary)
            .args(&args)
            .current_dir(&bin_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match spawn_result {
            Ok(mut child) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} started (pid {:?})", child.id()));
                let stderr = child.stderr.take();
                // Keep child alive while stderr streams
                let _child = child;
                stream_stderr(stderr, display_name, event_log.clone()).await;
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} process ended"));
            }
            Err(e) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} failed to start: {e}"));
            }
        }
    });
}

async fn stream_stderr(
    stderr: Option<tokio::process::ChildStderr>,
    label: &str,
    event_log: SharedEventLog,
) {
    use tokio::io::AsyncBufReadExt;
    if let Some(stderr) = stderr {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if !line.is_empty() {
                monitor::push_event(&event_log, "[ADAPTER]", format!("{label}: {line}"));
            }
        }
    }
}

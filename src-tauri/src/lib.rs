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

            spawn_sidecars(config.clone(), event_log.clone(), chat_child.clone());

            // Run requirements check at every startup and emit the result so the
            // frontend can show a fix prompt even when the wizard is already done.
            let handle_deps = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                // Small delay so the UI is ready to receive the event
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                if let Ok(deps) = commands::check_system_deps().await {
                    let _ = handle_deps.emit("system_deps_checked", &deps);
                }
            });

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

            let sched_loop = scheduler.clone();
            let handle_sched = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                orchestrator::scheduler::run_scheduler_loop(sched_loop, handle_sched).await;
            });

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
            commands::check_system_deps,
            commands::install_vcredist,
            commands::pick_model_file,
            commands::download_required_models,
            commands::send_message,
            commands::swap_model,
            commands::clear_model,
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

pub fn binaries_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = match cwd.file_name().and_then(|n| n.to_str()) {
        Some("src-tauri") => cwd.parent().unwrap_or(&cwd).to_path_buf(),
        _ => cwd,
    };
    root.join("binaries")
}

pub fn sidecar_filename(name: &str) -> String {
    #[cfg(target_os = "windows")]
    return format!("{name}-x86_64-pc-windows-msvc.exe");
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") { return format!("{name}-aarch64-apple-darwin"); }
        return format!("{name}-x86_64-apple-darwin");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return format!("{name}-x86_64-unknown-linux-gnu");
}

/// Build a tokio::process::Command that prioritises our bundled DLLs on Windows.
/// By prepending `bin_dir` to PATH we ensure our ggml/llama DLLs are found
/// before any conflicting versions from LM Studio, CUDA installers, etc.
fn make_cmd(binary: &PathBuf, bin_dir: &PathBuf) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.current_dir(bin_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Prepend our binaries/ to PATH so Windows DLL search finds our versions first
    let current_path = std::env::var("PATH").unwrap_or_default();
    let priority_path = format!("{};{}", bin_dir.display(), current_path);
    cmd.env("PATH", priority_path);

    cmd
}

pub fn start_chat_server(
    model_path: String,
    port: u16,
    event_log: SharedEventLog,
    chat_child: SharedChatChild,
) {
    tauri::async_runtime::spawn(async move {
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
                format!("llama (chat) binary not found at {}", binary.display()));
            return;
        }

        monitor::push_event(&event_log, "[ADAPTER]",
            format!("llama (chat) launching → {}", binary.display()));

        let spawn_result = make_cmd(&binary, &bin_dir)
            .args(["--model", &model_path,
                   "--port", &port.to_string(),
                   "--host", "127.0.0.1",
                   "--ctx-size", "4096",
                   "-ngl", "999"])
            .spawn();

        match spawn_result {
            Ok(mut child) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("llama (chat) started (pid {:?})", child.id()));
                let stderr = child.stderr.take();
                let stdout = child.stdout.take();
                { *chat_child.lock().await = Some(child); }

                // Stream both stdout and stderr
                tokio::join!(
                    stream_output(stdout, "llama (chat)", event_log.clone()),
                    stream_output(stderr, "llama (chat)", event_log.clone()),
                );

                let mut guard = chat_child.lock().await;
                if let Some(mut c) = guard.take() {
                    if let Ok(s) = c.wait().await {
                        let code = s.code().unwrap_or(-1);
                        let desc = exit_code_description(code);
                        monitor::push_event(&event_log, "[ADAPTER]",
                            format!("llama (chat) exited (code {code} — {desc})"));
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

fn spawn_sidecars(config: SharedConfig, event_log: SharedEventLog, chat_child: SharedChatChild) {
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

        // Embeddings: newer llama.cpp exposes /v1/embeddings on all instances by default.
        // --embedding flag removed — caused early exit on b9008+.
        spawn_direct("llama-server", "llama (embed)",
            vec!["--model".into(), embed_path,
                 "--port".into(), embed_port.to_string(),
                 "--host".into(), "127.0.0.1".into(),
                 "--ctx-size".into(), "512".into(),
                 "-ngl".into(), "999".into()],
            event_log.clone());

        // whisper-server uses -p for port (not --port)
        spawn_direct("whisper-server", "whisper",
            vec!["-m".into(), whisper_path,
                 "-p".into(), whisper_port.to_string()],
            event_log.clone());

        spawn_direct("kokoro-server", "kokoro",
            vec!["--port".into(), kokoro_port.to_string(),
                 "--host".into(), "127.0.0.1".into()],
            event_log.clone());
    });
}

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

        let spawn_result = make_cmd(&binary, &bin_dir).args(&args).spawn();

        match spawn_result {
            Ok(mut child) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} started (pid {:?})", child.id()));
                let stderr = child.stderr.take();
                let stdout = child.stdout.take();

                tokio::join!(
                    stream_output(stdout, display_name, event_log.clone()),
                    stream_output(stderr, display_name, event_log.clone()),
                );

                // Log exit code so we can diagnose crashes for ALL processes, not just chat
                if let Ok(status) = child.wait().await {
                    let code = status.code().unwrap_or(-1);
                    let desc = exit_code_description(code);
                    monitor::push_event(&event_log, "[ADAPTER]",
                        format!("{display_name} exited (code {code} — {desc})"));
                }
            }
            Err(e) => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} failed to start: {e}"));
            }
        }
    });
}

/// Stream either stdout or stderr to the event log, concurrently.
async fn stream_output<R>(reader: Option<R>, label: &str, event_log: SharedEventLog)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;
    if let Some(reader) = reader {
        let mut lines = tokio::io::BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if !line.is_empty() {
                monitor::push_event(&event_log, "[ADAPTER]", format!("{label}: {line}"));
            }
        }
    }
}

/// Human-readable descriptions for common Windows process exit codes.
fn exit_code_description(code: i32) -> &'static str {
    match code as u32 {
        0xC0000135 => "STATUS_DLL_NOT_FOUND — a required DLL is missing from binaries/",
        0xC0000139 => "STATUS_ENTRYPOINT_NOT_FOUND — DLL version mismatch (try: winget install Microsoft.VCRedist.2015+.x64)",
        0xC0000005 => "STATUS_ACCESS_VIOLATION — crash/segfault",
        0xC000007B => "STATUS_INVALID_IMAGE_FORMAT — wrong architecture (need x64)",
        0 => "clean exit",
        _ => "unknown",
    }
}

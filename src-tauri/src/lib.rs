mod audio;
mod commands;
mod config;
mod memory;
mod monitor;
mod orchestrator;

use config::AppConfig;
use monitor::{new_event_log, run_monitor_loop, SharedEventLog};
use orchestrator::{scheduler::ProactivityScheduler, Orchestrator};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;
use tokio::sync::{Mutex, RwLock};

pub type SharedConfig = Arc<RwLock<AppConfig>>;
pub type SharedOrchestrator = Arc<Mutex<Option<Orchestrator>>>;
pub type SharedScheduler = Arc<Mutex<ProactivityScheduler>>;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let config: SharedConfig = Arc::new(RwLock::new(AppConfig::default()));
            let orchestrator: SharedOrchestrator = Arc::new(Mutex::new(None));
            let scheduler: SharedScheduler =
                Arc::new(Mutex::new(ProactivityScheduler::new()));
            let event_log: SharedEventLog = new_event_log();

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());
            app.manage(event_log.clone());

            // ── Sidecar processes ─────────────────────────────────────────────
            spawn_sidecars(app.handle().clone(), config.clone(), event_log.clone());

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

/// Spawn all four sidecar processes.
/// Each process is kept alive by a dedicated tokio task that consumes its event stream.
/// Missing binaries are skipped gracefully — the app still runs, sidecars will show red in
/// the debug panel until the binary is present.
fn spawn_sidecars(
    app_handle: tauri::AppHandle,
    config: SharedConfig,
    event_log: SharedEventLog,
) {
    tauri::async_runtime::spawn(async move {
        let cfg = config.read().await;
        let models_dir = cfg.models_dir.to_str().unwrap_or("models").to_string();
        let chat_model     = cfg.chat_model.clone();
        let embed_file     = cfg.embed_model_file.clone();
        let whisper_file   = cfg.whisper_model_file.clone();
        let llama_port     = cfg.llama_port.to_string();
        let embed_port     = cfg.embed_port.to_string();
        let whisper_port   = cfg.whisper_port.to_string();
        let kokoro_port    = cfg.kokoro_port.to_string();
        drop(cfg);

        // ── llama-server: chat model ──────────────────────────────────────────
        if chat_model.is_empty() {
            monitor::push_event(&event_log, "[ADAPTER]", "chat_model not set — llama-server (chat) not started");
        } else {
            let model_path = format!("{models_dir}/{chat_model}");
            spawn_one(
                &app_handle, &event_log, "llama-server", "llama (chat)",
                &["--model", &model_path, "--port", &llama_port,
                  "--host", "127.0.0.1", "--ctx-size", "4096", "-ngl", "999"],
            );
        }

        // ── llama-server: embedding model ─────────────────────────────────────
        let embed_path = format!("{models_dir}/{embed_file}");
        spawn_one(
            &app_handle, &event_log, "llama-server", "llama (embed)",
            &["--model", &embed_path, "--port", &embed_port,
              "--host", "127.0.0.1", "--ctx-size", "512", "-ngl", "999", "--embedding"],
        );

        // ── whisper-server ────────────────────────────────────────────────────
        let whisper_path = format!("{models_dir}/{whisper_file}");
        spawn_one(
            &app_handle, &event_log, "whisper-server", "whisper",
            &["-m", &whisper_path, "--port", &whisper_port, "--host", "127.0.0.1"],
        );

        // ── kokoro-server ─────────────────────────────────────────────────────
        spawn_one(
            &app_handle, &event_log, "kokoro-server", "kokoro",
            &["--port", &kokoro_port, "--host", "127.0.0.1"],
        );
    });
}

fn spawn_one(
    app_handle: &tauri::AppHandle,
    event_log: &SharedEventLog,
    sidecar_name: &'static str,
    display_name: &'static str,
    args: &[&str],
) {
    let result = app_handle
        .shell()
        .sidecar(sidecar_name)
        .and_then(|cmd| cmd.args(args).spawn());

    match result {
        Ok((mut rx, child)) => {
            monitor::push_event(event_log, "[ADAPTER]", format!("{display_name} started"));
            // Keep the child alive and forward its stderr to the event log
            let log = event_log.clone();
            tauri::async_runtime::spawn(async move {
                let _child = child; // drop = kill; keep alive until stream closes
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stderr(line) => {
                            let text = String::from_utf8_lossy(&line).trim().to_string();
                            if !text.is_empty() {
                                monitor::push_event(
                                    &log,
                                    "[ADAPTER]",
                                    format!("{display_name}: {text}"),
                                );
                            }
                        }
                        CommandEvent::Error(e) => {
                            monitor::push_event(
                                &log,
                                "[ADAPTER]",
                                format!("{display_name} error: {e}"),
                            );
                        }
                        CommandEvent::Terminated(status) => {
                            monitor::push_event(
                                &log,
                                "[ADAPTER]",
                                format!("{display_name} exited: code {:?}", status.code),
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            });
        }
        Err(e) => {
            // Binary missing or not registered — non-fatal during development
            monitor::push_event(
                event_log,
                "[ADAPTER]",
                format!("{display_name} not started: {e}"),
            );
        }
    }
}

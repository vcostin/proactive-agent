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
pub type SharedChatChild = Arc<Mutex<Option<tauri_plugin_shell::process::CommandChild>>>;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // ── Resolve OS-appropriate data directory ─────────────────────────
            let data_dir = app.path()
                .app_data_dir()
                .expect("cannot resolve app data dir");
            let config_path = app.path()
                .app_config_dir()
                .expect("cannot resolve app config dir")
                .join("config.json");

            std::fs::create_dir_all(&data_dir).ok();

            // Load persisted config (or defaults seeded with data_dir)
            let cfg = AppConfig::load(&config_path, data_dir);

            // Ensure model/db directories exist
            std::fs::create_dir_all(&cfg.models_dir).ok();
            std::fs::create_dir_all(&cfg.db_path).ok();

            let config: SharedConfig = Arc::new(RwLock::new(cfg));
            let orchestrator: SharedOrchestrator = Arc::new(Mutex::new(None));
            let scheduler: SharedScheduler =
                Arc::new(Mutex::new(ProactivityScheduler::new()));
            let event_log: SharedEventLog = new_event_log();
            let chat_child: SharedChatChild = Arc::new(Mutex::new(None));

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());
            app.manage(event_log.clone());
            app.manage(chat_child.clone());

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

/// Start (or restart) the chat llama-server with a new model path.
/// Kills any previously running instance first, waits for the port to free,
/// then spawns the new process. Safe to call from `swap_model`.
pub fn start_chat_server(
    app_handle: &tauri::AppHandle,
    model_path: String,
    port: u16,
    event_log: SharedEventLog,
    chat_child: SharedChatChild,
) {
    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        // Kill old process, releasing the port
        {
            let mut guard = chat_child.lock().await;
            if let Some(old) = guard.take() {
                let _ = old.kill();
            }
        } // guard dropped before await
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let result = app_handle
            .shell()
            .sidecar("llama-server")
            .and_then(|cmd| {
                cmd.args([
                    "--model", &model_path,
                    "--port", &port.to_string(),
                    "--host", "127.0.0.1",
                    "--ctx-size", "4096",
                    "-ngl", "999",
                ])
                .spawn()
            });

        match result {
            Ok((mut rx, child)) => {
                monitor::push_event(&event_log, "[ADAPTER]", "llama (chat) started");
                {
                    let mut guard = chat_child.lock().await;
                    *guard = Some(child);
                } // guard dropped before event loop
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stderr(b) => {
                            let t = String::from_utf8_lossy(&b).trim().to_string();
                            if !t.is_empty() {
                                monitor::push_event(&event_log, "[ADAPTER]", format!("llama (chat): {t}"));
                            }
                        }
                        CommandEvent::Error(e) => {
                            monitor::push_event(&event_log, "[ADAPTER]", format!("llama (chat) error: {e}"));
                        }
                        CommandEvent::Terminated(s) => {
                            monitor::push_event(&event_log, "[ADAPTER]", format!("llama (chat) exited (code {:?})", s.code));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                monitor::push_event(&event_log, "[ADAPTER]", format!("llama (chat) not started: {e}"));
            }
        }
    });
}

fn spawn_sidecars(
    app_handle: tauri::AppHandle,
    config: SharedConfig,
    event_log: SharedEventLog,
) {
    tauri::async_runtime::spawn(async move {
        let cfg = config.read().await;
        let chat_model     = cfg.chat_model.clone();
        let embed_path     = cfg.embed_model_path().to_string_lossy().into_owned();
        let whisper_path   = cfg.whisper_model_path().to_string_lossy().into_owned();
        let llama_port     = cfg.llama_port.to_string();
        let embed_port     = cfg.embed_port.to_string();
        let whisper_port   = cfg.whisper_port.to_string();
        let kokoro_port    = cfg.kokoro_port.to_string();
        drop(cfg);

        if chat_model.is_empty() {
            monitor::push_event(&event_log, "[ADAPTER]", "chat model not configured — llama-server (chat) not started");
        } else {
            spawn_one(
                &app_handle, &event_log, "llama-server", "llama (chat)",
                &["--model", &chat_model, "--port", &llama_port,
                  "--host", "127.0.0.1", "--ctx-size", "4096", "-ngl", "999"],
            );
        }

        spawn_one(
            &app_handle, &event_log, "llama-server", "llama (embed)",
            &["--model", &embed_path, "--port", &embed_port,
              "--host", "127.0.0.1", "--ctx-size", "512", "-ngl", "999", "--embedding"],
        );

        spawn_one(
            &app_handle, &event_log, "whisper-server", "whisper",
            &["-m", &whisper_path, "--port", &whisper_port, "--host", "127.0.0.1"],
        );

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
            let log = event_log.clone();
            tauri::async_runtime::spawn(async move {
                let _child = child;
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stderr(line) => {
                            let text = String::from_utf8_lossy(&line).trim().to_string();
                            if !text.is_empty() {
                                monitor::push_event(&log, "[ADAPTER]", format!("{display_name}: {text}"));
                            }
                        }
                        CommandEvent::Error(e) => {
                            monitor::push_event(&log, "[ADAPTER]", format!("{display_name} error: {e}"));
                        }
                        CommandEvent::Terminated(s) => {
                            monitor::push_event(&log, "[ADAPTER]", format!("{display_name} exited (code {:?})", s.code));
                            break;
                        }
                        _ => {}
                    }
                }
            });
        }
        Err(e) => {
            monitor::push_event(event_log, "[ADAPTER]", format!("{display_name} not started: {e}"));
        }
    }
}

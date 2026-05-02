mod audio;
mod commands;
mod config;
mod memory;
mod monitor;
mod orchestrator;

use config::AppConfig;
use orchestrator::{scheduler::ProactivityScheduler, Orchestrator};
use std::sync::Arc;
use tauri::{Emitter, Manager};
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

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());

            // Async-initialise the orchestrator (opens LanceDB, connects to llama.cpp)
            let orch_init = orchestrator.clone();
            let cfg_init = config.clone();
            let handle_init = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                match Orchestrator::new(cfg_init).await {
                    Ok(o) => {
                        *orch_init.lock().await = Some(o);
                        let _ = handle_init.emit("orchestrator_ready", ());
                    }
                    Err(e) => {
                        eprintln!("[SETUP] orchestrator init failed: {e}");
                        let _ = handle_init.emit("init_error", e.to_string());
                    }
                }
            });

            // Proactivity scheduler — runs independently of orchestrator
            let sched_loop = scheduler.clone();
            let handle_sched = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                orchestrator::scheduler::run_scheduler_loop(sched_loop, handle_sched)
                    .await;
            });

            // EXTEND: Phase 4 — spawn sidecar processes (llama-server, whisper, kokoro)
            // EXTEND: Phase 4 — start monitor health-check polling task

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

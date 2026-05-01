mod audio;
mod commands;
mod config;
mod memory;
mod monitor;
mod orchestrator;

use config::AppConfig;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

pub type SharedConfig = Arc<RwLock<AppConfig>>;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let config = Arc::new(RwLock::new(AppConfig::default()));
            app.manage(config);

            // EXTEND: spawn llama-server, whisper-server, kokoro-server sidecars here
            // EXTEND: start ProactivityScheduler tokio task (Phase 3)
            // EXTEND: start monitor health-check polling task (Phase 4)

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

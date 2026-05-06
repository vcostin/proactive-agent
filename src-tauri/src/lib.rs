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
/// Stop signal for the voice capture thread. None = not recording.
pub type SharedVoiceStop = Arc<std::sync::Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>>;
/// PIDs of all spawned sidecar processes — killed on app exit so DLLs are released.
pub type SharedProcessPids = Arc<std::sync::Mutex<Vec<u32>>>;
/// Live microphone energy (RMS as f32 bits) — updated by the capture thread, read by UI.
pub type SharedAudioEnergy = Arc<std::sync::atomic::AtomicU32>;

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
            let voice_stop: SharedVoiceStop = Arc::new(std::sync::Mutex::new(None));
            let process_pids: SharedProcessPids = Arc::new(std::sync::Mutex::new(Vec::new()));
            let audio_energy: SharedAudioEnergy = Arc::new(std::sync::atomic::AtomicU32::new(0));

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());
            app.manage(event_log.clone());
            app.manage(chat_child.clone());
            app.manage(voice_stop.clone());
            app.manage(process_pids.clone());
            app.manage(audio_energy.clone());

            spawn_sidecars(config.clone(), event_log.clone(), chat_child.clone(), process_pids.clone());

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

            // ── Semantic distillation loop (every 10 min) ────────────────────
            let orch_distill = orchestrator.clone();
            let log_distill = event_log.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await; // wait for init
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
                loop {
                    interval.tick().await;
                    let mut lock = orch_distill.lock().await;
                    if let Some(ref mut orch) = *lock {
                        match orch.distill().await {
                            Ok(n) if n > 0 => monitor::push_event(&log_distill, "[MEMORY]",
                                format!("distilled {n} new semantic facts")),
                            Ok(_) => {}
                            Err(e) => monitor::push_event(&log_distill, "[MEMORY]",
                                format!("distillation error: {e}")),
                        }
                    }
                }
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
            commands::open_llama_diagnostic,
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
            commands::get_gen_settings,
            commands::set_gen_settings,
            commands::test_defer,
            commands::diagnose_chat_server,
            commands::start_voice_input,
            commands::stop_voice_input,
            commands::get_audio_energy,
        ])
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                kill_all_sidecars(app_handle);
            }
        });
}

/// Kill every tracked sidecar process so DLLs are released and ports freed.
/// Called on app exit — ensures `npm run setup` works without closing manually.
fn kill_all_sidecars(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    // Kill chat server via stored child handle
    if let Some(pids) = app_handle.try_state::<SharedProcessPids>() {
        if let Ok(list) = pids.inner().lock() {
            for &pid in list.iter() {
                if pid == 0 { continue; }
                #[cfg(target_os = "windows")]
                {
                    // /F = force, /T = kill child tree
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .spawn();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .spawn();
                }
            }
        }
    }

    // Also kill the chat server via its stored child handle
    if let Some(chat) = app_handle.try_state::<SharedChatChild>() {
        if let Ok(mut guard) = chat.inner().try_lock() {
            if let Some(ref mut child) = *guard {
                if let Some(pid) = child.id() {
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .spawn();
                    #[cfg(not(target_os = "windows"))]
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .spawn();
                }
            }
        }
    }
}

// ── Process helpers ───────────────────────────────────────────────────────────

pub fn binaries_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        // Dev: current_dir() is src-tauri/ — step up to project root
        let cwd = std::env::current_dir().unwrap_or_default();
        let root = match cwd.file_name().and_then(|n| n.to_str()) {
            Some("src-tauri") => cwd.parent().unwrap_or(&cwd).to_path_buf(),
            _ => cwd,
        };
        return root.join("binaries");
    }
    // Release: Tauri bundles externalBin alongside the main exe
    #[allow(unreachable_code)]
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .map(|p| p.join("binaries"))
        .unwrap_or_else(|| PathBuf::from("binaries"))
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

/// Locate a sidecar binary. Checks an isolated subdirectory first so each
/// sidecar has its own DLLs and can't be contaminated by another sidecar's
/// older/newer versions of shared libraries like ggml.dll.
///
/// Search order:
///   1. binaries/{name}/{name}-{target}.exe   ← isolated (preferred)
///   2. binaries/{name}-{target}.exe          ← legacy flat layout
pub fn find_sidecar(name: &str) -> Option<PathBuf> {
    let filename = sidecar_filename(name);
    let root = binaries_dir();
    // The short name is the first segment: "llama-server" → "llama", "parakeet-server" → "parakeet"
    let short = name.split('-').next().unwrap_or(name);
    [
        root.join(name).join(&filename),    // binaries/llama-server/llama-server-...exe
        root.join(short).join(&filename),   // binaries/llama/llama-server-...exe  ← current layout
        root.join(&filename),               // binaries/llama-server-...exe  (legacy)
    ]
    .into_iter()
    .find(|p| p.exists() && p.metadata().map(|m| m.len() > 1024).unwrap_or(false))
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
    pids: SharedProcessPids,
) {
    tauri::async_runtime::spawn(async move {
        {
            let mut guard = chat_child.lock().await;
            if let Some(mut old) = guard.take() {
                let _ = old.kill().await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        let binary = match find_sidecar("llama-server") {
            Some(b) => b,
            None => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    "llama (chat): binary not found — run: npm run setup");
                return;
            }
        };
        let dll_dir = binary.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(binaries_dir);

        monitor::push_event(&event_log, "[ADAPTER]",
            format!("llama (chat) launching → {}", binary.display()));

        let spawn_result = make_cmd(&binary, &dll_dir)
            .args(["--model", &model_path,
                   "--port", &port.to_string(),
                   "--host", "127.0.0.1",
                   "--ctx-size", "4096",
                   "-ngl", "999",
                   "--alias", "llama-chat"])
            .spawn();

        match spawn_result {
            Ok(mut child) => {
                let pid = child.id().unwrap_or(0);
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("llama (chat) started (pid {pid})"));
                // Register PID for graceful shutdown
                if let Ok(mut list) = pids.lock() { list.push(pid); }
                let stderr = child.stderr.take();
                let stdout = child.stdout.take();
                { *chat_child.lock().await = Some(child); }

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

fn spawn_sidecars(config: SharedConfig, event_log: SharedEventLog, chat_child: SharedChatChild, pids: SharedProcessPids) {
    tauri::async_runtime::spawn(async move {
        let cfg = config.read().await;
        let chat_model     = cfg.chat_model.clone();
        let embed_path     = cfg.embed_model_path().to_string_lossy().into_owned();
        let cfg_models_dir = cfg.models_dir.clone();
        let embed_port     = cfg.embed_port;
        let _stt_port      = cfg.stt_port; // port is hardcoded in parakeet-server binary
        let kokoro_port    = cfg.kokoro_port;
        let llama_port     = cfg.llama_port;
        drop(cfg);

        if chat_model.is_empty() {
            monitor::push_event(&event_log, "[ADAPTER]",
                "chat model not configured — pick one in the Models tab");
        } else {
            start_chat_server(chat_model, llama_port, event_log.clone(), chat_child, pids.clone());
        }

        spawn_direct("llama-server", "llama (embed)",
            vec!["--model".into(), embed_path,
                 "--port".into(), embed_port.to_string(),
                 "--host".into(), "127.0.0.1".into(),
                 "--ctx-size".into(), "512".into(),
                 "-ngl".into(), "999".into(),
                 "--embedding".into(),
                 "--alias".into(), "nomic-embed-text".into()],
            event_log.clone(), pids.clone());

        // Parakeet TDT STT server — listens on port 5092, loads model from HF cache
        spawn_direct("parakeet-server", "Parakeet STT",
            vec![],   // server uses hardcoded host/port from app.py
            event_log.clone(), pids.clone());

        // TTS via sherpa-onnx (piper voice model)
        let tts_model = cfg_models_dir.join("tts").join("en_US-lessac-medium.onnx");
        let _tts_tokens = cfg_models_dir.join("tts").join("en_US-lessac-medium.onnx.json"); // used by TtsClient subprocess
        // TTS (sherpa-onnx) runs as a subprocess per request, not as a persistent server.
        // TtsClient.synthesise_via_cli() calls the binary directly — no HTTP server needed.
        let tts_bin = crate::find_sidecar("kokoro-server");
        if tts_bin.is_some() && tts_model.exists() {
            monitor::push_event(&event_log, "[ADAPTER]", "TTS (sherpa-onnx) ready — subprocess mode");
        } else {
            monitor::push_event(&event_log, "[ADAPTER]", "TTS unavailable — run: npm run setup");
        }
        let _ = kokoro_port; // port unused in subprocess mode
    });
}

fn spawn_direct(
    binary_name: &'static str,
    display_name: &'static str,
    args: Vec<String>,
    event_log: SharedEventLog,
    pids: SharedProcessPids,
) {
    let binary = match find_sidecar(binary_name) {
        Some(b) => b,
        None => {
            monitor::push_event(&event_log, "[ADAPTER]",
                format!("{display_name}: not found — run: npm run setup"));
            return;
        }
    };
    let dll_dir = binary.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(binaries_dir);

    tauri::async_runtime::spawn(async move {
        monitor::push_event(&event_log, "[ADAPTER]",
            format!("{display_name} launching → {}", binary.display()));

        let spawn_result = make_cmd(&binary, &dll_dir).args(&args).spawn();

        match spawn_result {
            Ok(mut child) => {
                let pid = child.id().unwrap_or(0);
                monitor::push_event(&event_log, "[ADAPTER]",
                    format!("{display_name} started (pid {pid})"));
                // Register for graceful shutdown
                if let Ok(mut list) = pids.lock() { list.push(pid); }
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

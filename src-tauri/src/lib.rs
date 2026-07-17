pub mod audio;
pub mod binary_store;
mod commands;
pub mod constants;
mod config;
mod memory;
mod monitor;
mod orchestrator;
pub mod platform;
pub mod setup;

/// Re-export for ergonomic use across all sibling modules.
pub use constants::SIDECAR_HOST;

use config::AppConfig;
use monitor::{new_event_log, run_monitor_loop, SharedEventLog};
use orchestrator::{scheduler::ProactivityScheduler, Orchestrator};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex, RwLock};

pub type SharedConfig = Arc<RwLock<AppConfig>>;
pub type SharedOrchestrator = Arc<Mutex<Option<Orchestrator>>>;
pub type SharedScheduler = Arc<Mutex<ProactivityScheduler>>;
pub type SharedChatChild = Arc<Mutex<Option<tokio::process::Child>>>;
/// Stop signal for the voice capture thread. None = not recording.
pub type SharedVoiceStop = Arc<std::sync::Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>>;
/// Cancels in-flight Piper playback when a new speak/preview starts.
pub type SharedPlaybackGate = Arc<audio::PlaybackGate>;
/// PIDs of all spawned sidecar processes — killed on app exit so DLLs are released.
pub type SharedProcessPids = Arc<std::sync::Mutex<Vec<u32>>>;
/// Live microphone energy (RMS as f32 bits) — updated by the capture thread, read by UI.
pub type SharedAudioEnergy = Arc<std::sync::atomic::AtomicU32>;
/// In-process Host STT engine. `None` = soft-fail / not ready (Core agent still up).
pub type SharedSttEngine = Arc<std::sync::Mutex<Option<Arc<audio::SttClient>>>>;

pub fn run() {
    // Before any cpal/ALSA device probe (sidecars, TTS, mic).
    audio::quiet_backend_probe_noise();

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
            // Load persisted deferred queue; overdue items flush when the UI mounts
            // (invoke return path — avoids losing events before listeners attach).
            let queue_path = ProactivityScheduler::queue_path_beside_config(&config_path);
            let scheduler: SharedScheduler = Arc::new(Mutex::new(
                ProactivityScheduler::load(queue_path.clone()).unwrap_or_else(|e| {
                    eprintln!("[SCHEDULER] failed to load deferred queue: {e}");
                    ProactivityScheduler::with_persist(queue_path)
                }),
            ));
            let event_log: SharedEventLog = new_event_log();
            let chat_child: SharedChatChild = Arc::new(Mutex::new(None));
            let voice_stop: SharedVoiceStop = Arc::new(std::sync::Mutex::new(None));
            let playback_gate: SharedPlaybackGate = Arc::new(audio::PlaybackGate::new());
            let process_pids: SharedProcessPids = Arc::new(std::sync::Mutex::new(Vec::new()));
            let audio_energy: SharedAudioEnergy = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let stt_engine: SharedSttEngine = Arc::new(std::sync::Mutex::new(None));

            app.manage(config.clone());
            app.manage(orchestrator.clone());
            app.manage(scheduler.clone());
            app.manage(event_log.clone());
            app.manage(chat_child.clone());
            app.manage(voice_stop.clone());
            app.manage(playback_gate.clone());
            app.manage(process_pids.clone());
            app.manage(audio_energy.clone());
            app.manage(stt_engine.clone());

            // Host STT: soft-fail load — Core agent stays up; rich diagnostics on failure.
            {
                let log = event_log.clone();
                let handle = app_handle.clone();
                let engine_slot = stt_engine.clone();
                match try_load_stt_engine() {
                    Ok(client) => {
                        *engine_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(client);
                        monitor::push_event(
                            &log,
                            "[STT]",
                            "Host STT path ready (in-process ort, CPU)",
                        );
                    }
                    Err(e) => {
                        let msg = format!(
                            "Host STT soft-fail — Core agent continues; transcription off. \
                             Open Setup Wizard / Setup repair.\n{e:#}"
                        );
                        monitor::push_event(&log, "[STT]", &msg);
                        let _ = handle.emit(
                            "debug_event",
                            monitor::DebugEvent {
                                timestamp: chrono::Utc::now(),
                                component: "[STT]".into(),
                                message: msg,
                            },
                        );
                    }
                }
            }

            spawn_sidecars(config.clone(), event_log.clone(), chat_child.clone(), process_pids.clone());

            // Run requirements check at every startup and emit the result so the
            // frontend can show a fix prompt even when the wizard is already done.
            let handle_deps = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                // Small delay so the UI is ready to receive the event
                tokio::time::sleep(std::time::Duration::from_millis(constants::DEPS_CHECK_STARTUP_DELAY_MS)).await;
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
                tokio::time::sleep(std::time::Duration::from_secs(constants::DISTILLATION_STARTUP_DELAY_SECS)).await;
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(constants::DISTILLATION_INTERVAL_SECS));
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
            commands::check_binaries_ready,
            commands::download_required_binaries,
            commands::verify_platform_artifacts,
            commands::get_artifact_catalog,
            commands::check_system_deps,
            commands::install_vcredist,
            #[cfg(debug_assertions)] commands::open_llama_diagnostic,
            commands::pick_model_file,
            commands::download_required_models,
            commands::download_curated_voice,
            commands::list_curated_voices,
            commands::get_tts_voice,
            commands::set_tts_voice,
            commands::preview_voice,
            commands::send_message,
            commands::swap_model,
            commands::clear_model,
            commands::get_memories,
            commands::get_system_status,
            commands::get_last_context,
            commands::fire_deferred_now,
            commands::cancel_deferred,
            commands::flush_due_deferred,
            commands::list_models,
            commands::get_debug_events,
            commands::get_gen_settings,
            commands::set_gen_settings,
            #[cfg(debug_assertions)] commands::test_defer,
            #[cfg(debug_assertions)] commands::diagnose_chat_server,
            commands::speak_text,
            commands::reset_chat,
            commands::start_voice_input,
            commands::stop_voice_input,
            commands::get_audio_energy,
        ])
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|app_handle, event| {
            // Kill early on ExitRequested (before tear-down). Exit alone can be too
            // late, and async-spawned `kill` kids often die with the app before running.
            match event {
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                    kill_all_sidecars(app_handle);
                }
                tauri::RunEvent::WindowEvent { event, .. } => {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        kill_all_sidecars(app_handle);
                    }
                }
                _ => {}
            }
        });
}

/// Kill every tracked sidecar (and its process group on Unix) so ports/VRAM are freed.
fn kill_all_sidecars(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    let mut seen = std::collections::HashSet::new();

    if let Some(pids) = app_handle.try_state::<SharedProcessPids>() {
        if let Ok(list) = pids.inner().lock() {
            for &pid in list.iter() {
                if pid == 0 || !seen.insert(pid) {
                    continue;
                }
                kill_process_tree(pid);
            }
        }
    }

    if let Some(chat) = app_handle.try_state::<SharedChatChild>() {
        if let Ok(mut guard) = chat.inner().try_lock() {
            if let Some(mut child) = guard.take() {
                if let Some(pid) = child.id() {
                    if seen.insert(pid) {
                        kill_process_tree(pid);
                    }
                }
                let _ = child.start_kill();
            }
        }
    }

    // Safety net: free known sidecar ports even if PID tracking missed a fork/exec.
    // Reads config when available; falls back to defaults. (STT is in-process — no port.)
    let (chat_port, embed_port) = app_handle
        .try_state::<SharedConfig>()
        .and_then(|cfg| cfg.inner().try_read().ok().map(|c| (c.llama_port, c.embed_port)))
        .unwrap_or((18080, 18081));
    for port in [chat_port, embed_port] {
        kill_listeners_on_port(port);
    }
}

/// Synchronously kill `pid` and (on Unix) its process group.
fn kill_process_tree(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // Negative PGID → entire group (set via process_group(0) at spawn).
        let pgid = pid as i32;
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
            libc::kill(pgid, libc::SIGKILL);
        }
    }
}

/// Best-effort: terminate whoever is listening on `port` (Linux/macOS).
fn kill_listeners_on_port(port: u16) {
    #[cfg(target_os = "windows")]
    {
        // Prefer tracked PIDs; port resolution via netstat is brittle here.
        let _ = port;
    }
    #[cfg(unix)]
    {
        // fuser is common on Arch; ignore failures if missing.
        let _ = std::process::Command::new("fuser")
            .args(["-k", "-TERM", &format!("{port}/tcp")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = std::process::Command::new("fuser")
            .args(["-k", "-KILL", &format!("{port}/tcp")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

// ── Process helpers ───────────────────────────────────────────────────────────

/// In dev: project_root/binaries/ — populated by `npm run setup`.
/// In release: %APPDATA%\com.proactive.agent\binaries\ (user-writable).
///   The wizard downloads llama-server and piper here at first run.
///   Parakeet is bundled by the installer and found via find_sidecar's
///   exe-directory fallback, not through binaries_dir().
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
    // Release: wizard-downloaded binaries live in AppData (user-writable,
    // no admin rights needed unlike Program Files).
    #[allow(unreachable_code)]
    release_binaries_dir()
}

/// Platform-specific AppData path for release binaries.
/// Mirrors the path Tauri uses for its own app data (same identifier).
#[cfg(not(debug_assertions))]
fn release_binaries_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_default();
    #[cfg(target_os = "macos")]
    let base = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join("Library/Application Support"))
        .unwrap_or_default();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".local/share"))
        .unwrap_or_default();
    base.join("com.proactive.agent").join("binaries")
}

#[cfg(debug_assertions)]
fn release_binaries_dir() -> PathBuf { unreachable!() }

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

/// Locate a sidecar binary.
///
/// Search order (dev):
///   1. binaries/{short}/{filename}  ← npm run setup layout
///   2. binaries/{filename}          ← legacy flat
///
/// Search order (release):
///   1. AppData/com.proactive.agent/binaries/{short}/{filename}  ← wizard-downloaded
///   2. AppData/.../binaries/{filename}
///   3. {exe_dir}/{short}/{filename}  ← installer-bundled (e.g. parakeet)
///   4. {exe_dir}/{filename}
pub fn find_sidecar(name: &str) -> Option<PathBuf> {
    let filename = sidecar_filename(name);
    let root = binaries_dir();
    let short = name.split('-').next().unwrap_or(name);

    // Release: also check exe directory for installer-bundled binaries (e.g. parakeet)
    #[cfg(not(debug_assertions))]
    let exe_candidates: Vec<PathBuf> = std::env::current_exe().ok()
        .and_then(|exe| exe.parent().map(|d| d.to_path_buf()))
        .map(|exe_dir| vec![
            exe_dir.join(short).join(&filename),
            exe_dir.join(name).join(&filename),
            exe_dir.join(&filename),
        ])
        .unwrap_or_default();

    #[cfg(debug_assertions)]
    let exe_candidates: Vec<PathBuf> = vec![];

    let candidates: Vec<PathBuf> = [
        root.join(short).join(&filename),
        root.join(name).join(&filename),
        root.join(&filename),
    ]
    .into_iter()
    .chain(exe_candidates)
    .collect();

    candidates.into_iter().find(|p| sidecar_file_usable(p))
}

/// Accept real binaries and small shell launchers (Linux Parakeet wrapper is ~700 B).
/// The old `len > 1024` gate rejected the managed Linux launcher as "not found".
pub fn sidecar_file_usable(p: &Path) -> bool {
    let Ok(meta) = p.metadata() else { return false };
    if !meta.is_file() { return false; }
    let len = meta.len();
    #[cfg(target_os = "windows")]
    {
        // Skip empty / placeholder stubs that sometimes appear in binaries/
        len > 1024
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        len > 32 && (meta.permissions().mode() & 0o111) != 0
    }
}

/// Build a tokio::process::Command that finds bundled native libs next to the binary.
/// Windows: prepends `bin_dir` to PATH (DLL search).
/// Linux/macOS: prepends `bin_dir` to LD_LIBRARY_PATH / DYLD_LIBRARY_PATH.
fn make_cmd(binary: &PathBuf, bin_dir: &PathBuf) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.current_dir(bin_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Own process group so exit can SIGKILL the whole tree (uvicorn workers, etc.).
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    #[cfg(target_os = "windows")]
    {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{}", bin_dir.display(), current_path));
    }
    #[cfg(target_os = "linux")]
    {
        let current = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let path = if current.is_empty() {
            bin_dir.display().to_string()
        } else {
            format!("{}:{}", bin_dir.display(), current)
        };
        cmd.env("LD_LIBRARY_PATH", path);
    }
    #[cfg(target_os = "macos")]
    {
        let current = std::env::var("DYLD_LIBRARY_PATH").unwrap_or_default();
        let path = if current.is_empty() {
            bin_dir.display().to_string()
        } else {
            format!("{}:{}", bin_dir.display(), current)
        };
        cmd.env("DYLD_LIBRARY_PATH", path);
    }

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
        tokio::time::sleep(std::time::Duration::from_millis(constants::CHAT_SERVER_RESTART_DELAY_MS)).await;

        let binary = match find_sidecar("llama-server") {
            Some(b) => b,
            None => {
                monitor::push_event(&event_log, "[ADAPTER]",
                    "llama (chat): binary not found — open Setup repair");
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
                   "--host", SIDECAR_HOST,
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
                 "--host".into(), SIDECAR_HOST.into(),
                 "--ctx-size".into(), "512".into(),
                 "-ngl".into(), "999".into(),
                 "--embedding".into(),
                 "--alias".into(), "nomic-embed-text".into()],
            event_log.clone(), pids.clone());

        // Host STT is in-process ort — no Parakeet HTTP sidecar.

        // TTS uses Piper as a subprocess per request — no persistent server needed.
        let tts_model = cfg_models_dir.join("tts").join(constants::TTS_MODEL_FILE);
        if tts_model.exists() {
            monitor::push_event(&event_log, "[ADAPTER]", "TTS (Piper) ready — subprocess mode");
        } else {
            monitor::push_event(&event_log, "[ADAPTER]", "TTS unavailable — open Setup repair");
        }
    });
}

fn try_load_stt_engine() -> anyhow::Result<Arc<audio::SttClient>> {
    let model_dir = AppConfig::stt_model_dir();
    let ort_dir = AppConfig::ort_lib_dir();
    let client = audio::SttClient::new(&model_dir, &ort_dir)?;
    Ok(Arc::new(client))
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
                format!("{display_name}: not found — open Setup repair"));
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

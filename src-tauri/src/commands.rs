use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::monitor::{AudioState, MemoryStats, ModelInfo, SystemStatus};
use crate::orchestrator::context::AssembledContext;
use crate::monitor::SharedEventLog;
use crate::{SharedAudioEnergy, SharedChatChild, SharedConfig, SharedOrchestrator, SharedProcessPids, SharedScheduler, SharedSttClient, SharedVoiceStop};

type CmdResult<T> = Result<T, String>;

fn to_cmd_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Emit a debug event from a synchronous context (e.g. std::thread).
/// AppHandle::emit is sync so this works without an async runtime.
fn debug_event_sync(app: &tauri::AppHandle, message: String) {
    use tauri::Emitter;
    let _ = app.emit("debug_event", crate::monitor::DebugEvent {
        timestamp: chrono::Utc::now(),
        component: "[AUDIO]".to_string(),
        message,
    });
}

/// Compare the SHA256 of `data` against a lowercase hex string.
/// Returns false if the hash doesn't match OR if `expected` is not valid 64-char hex.
fn verify_sha256(data: &[u8], expected_hex: &str) -> bool {
    use sha2::{Digest, Sha256};
    if expected_hex.len() != 64 { return false; }
    let digest = Sha256::digest(data);
    let actual = digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    });
    actual == expected_hex
}

// ── Setup / first-run ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupStatus {
    pub ready: bool,
    pub chat_model: String,
    pub embed_model_ready: bool,
    /// Parakeet TDT ONNX model files present in binaries/parakeet/models/
    pub stt_model_ready: bool,
    pub data_dir: String,
    /// Whether llama-server and piper binaries are present and ready.
    pub binaries: crate::binary_store::BinariesStatus,
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
        stt_model_ready: crate::config::AppConfig::stt_model_ready(),
        data_dir: cfg.models_dir
            .parent()
            .unwrap_or(&cfg.models_dir)
            .to_string_lossy()
            .into_owned(),
        binaries: crate::binary_store::check_binaries(),
    })
}

/// Check which sidecar binaries are present without triggering a download.
#[tauri::command]
pub async fn check_binaries_ready() -> CmdResult<crate::binary_store::BinariesStatus> {
    Ok(crate::binary_store::check_binaries())
}

/// Download llama-server and piper for the current OS/arch.
/// Emits `download_progress` events. Parakeet is excluded — no public release URL.
#[tauri::command]
pub async fn download_required_binaries(app_handle: tauri::AppHandle) -> CmdResult<()> {
    crate::binary_store::download_all(&app_handle)
        .await
        .map_err(to_cmd_err)
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

/// Download required models (nomic-embed-text + Parakeet TDT STT).
/// Emits `download_progress` events.
#[tauri::command]
pub async fn download_required_models(
    config: State<'_, SharedConfig>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let (models_dir, embed_path) = {
        let cfg = config.read().await;
        (cfg.models_dir.clone(), cfg.embed_model_path())
    };

    std::fs::create_dir_all(&models_dir).map_err(to_cmd_err)?;

    // Parakeet model files go into binaries/parakeet/models/
    let stt_model_dir = crate::config::AppConfig::stt_model_dir();
    std::fs::create_dir_all(&stt_model_dir).map_err(to_cmd_err)?;
    use crate::constants::*;
    let parakeet_onnx   = stt_model_dir.join(STT_MODEL_FILE);
    let parakeet_tokens = stt_model_dir.join(STT_TOKENS_FILE);

    let downloads: &[(&str, &str, &std::path::Path)] = &[
        (EMBED_MODEL_FILE, EMBED_MODEL_URL, &embed_path),
        (STT_MODEL_FILE,   STT_MODEL_URL,   &parakeet_onnx),
        (STT_TOKENS_FILE,  STT_TOKENS_URL,  &parakeet_tokens),
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

// ── Text-to-speech output ─────────────────────────────────────────────────────

/// Speak `text` through the default audio output using the sherpa-onnx TTS binary.
/// Fire-and-forget — returns immediately, audio plays in the background.
#[tauri::command]
pub async fn speak_text(
    text: String,
    event_log: State<'_, SharedEventLog>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    // Guard against unbounded piper stdin input
    const MAX_TTS_BYTES: usize = 4 * 1024;
    if text.len() > MAX_TTS_BYTES {
        return Err(format!("text too long for TTS ({} bytes, max {MAX_TTS_BYTES})", text.len()));
    }

    let log = event_log.inner().clone();
    crate::monitor::emit_debug_event(&app_handle, &log, "[AUDIO]",
        format!("TTS triggered ({} chars)", text.len())).await;
    tauri::async_runtime::spawn(async move {
        let client = crate::audio::tts::TtsClient::new(0);
        match client.speak(&text, &app_handle).await {
            Ok(()) => { crate::monitor::emit_debug_event(&app_handle, &log, "[AUDIO]", "TTS done").await; }
            Err(e) => { crate::monitor::emit_debug_event(&app_handle, &log, "[AUDIO]", format!("TTS failed: {e}")).await; }
        }
    });
    Ok(())
}

// ── Voice input ───────────────────────────────────────────────────────────────

/// Start microphone capture and STT loop.
/// cpal::Stream is !Send on WASAPI so we keep it on a dedicated std::thread.
/// Transcripts arrive as `voice_transcript` Tauri events.
/// Start (or restart) all sidecars after the wizard downloads binaries.
/// Called by the wizard after Step 1 completes so llama/embed start without
/// requiring the user to restart the app.
#[tauri::command]
pub async fn start_sidecars(
    config: State<'_, SharedConfig>,
    event_log: State<'_, SharedEventLog>,
    chat_child: State<'_, SharedChatChild>,
    process_pids: State<'_, SharedProcessPids>,
) -> CmdResult<()> {
    crate::spawn_sidecars(
        config.inner().clone(),
        event_log.inner().clone(),
        chat_child.inner().clone(),
        process_pids.inner().clone(),
    );
    Ok(())
}

/// (Re-)initialise the ort STT session after the wizard downloads the model.
/// Also called automatically at startup if the model is already present.
#[tauri::command]
pub async fn init_stt_client(stt_client: State<'_, SharedSttClient>) -> CmdResult<()> {
    use crate::constants::{STT_MODEL_FILE, STT_TOKENS_FILE};
    use crate::config::AppConfig;

    let model_path  = AppConfig::stt_model_dir().join(STT_MODEL_FILE);
    let tokens_path = AppConfig::stt_model_dir().join(STT_TOKENS_FILE);

    if !model_path.exists() || !tokens_path.exists() {
        return Err("STT model files not found — run the wizard to download them".to_string());
    }

    let client = crate::audio::stt::SttClient::new(&model_path, &tokens_path)
        .map_err(to_cmd_err)?;

    if let Ok(mut guard) = stt_client.inner().lock() {
        *guard = Some(Arc::new(client));
    }
    Ok(())
}

#[tauri::command]
pub async fn start_voice_input(
    voice_stop: State<'_, SharedVoiceStop>,
    audio_energy: State<'_, SharedAudioEnergy>,
    stt_client: State<'_, SharedSttClient>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Stop any existing recording
    if let Ok(mut g) = voice_stop.inner().lock() {
        if let Some(flag) = g.take() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<f32>>(128);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();

    // Channel to get the actual sample rate/channels back from the audio thread
    let (cfg_tx, cfg_rx) = std::sync::mpsc::channel::<(u32, u16)>();
    let energy_arc = audio_energy.inner().clone();
    let app_for_thread = app_handle.clone();

    // audio capture: stays on its own thread (cpal::Stream is !Send on WASAPI)
    let thread_result = std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn(move || {
            match crate::audio::capture::AudioCapture::start(tx, energy_arc) {
                Ok(capture) => {
                    let sr = capture.sample_rate;
                    let ch = capture.channels;
                    debug_event_sync(&app_for_thread, format!(
                        "capture started: {sr} Hz, {ch} ch — device: {}",
                        capture.device_name
                    ));
                    // Send actual device config so STT loop uses the correct sample rate
                    let _ = cfg_tx.send((sr, ch));
                    while !stop_clone.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    debug_event_sync(&app_for_thread, "capture stopped".to_string());
                }
                Err(e) => eprintln!("[AUDIO] capture failed: {e}"),
            }
        });

    if let Err(e) = thread_result {
        return Err(format!("failed to spawn audio thread: {e}"));
    }

    // Store stop flag so stop_voice_input can signal the thread
    if let Ok(mut g) = voice_stop.inner().lock() {
        *g = Some(stop_flag);
    }

    // Wait briefly for the capture thread to report its actual sample rate/channels
    let (sample_rate, channels) = cfg_rx
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or((16000, 1));

    // Get the STT client — returns error if model hasn't been downloaded yet
    let client = stt_client.inner().lock()
        .map_err(|_| "STT client mutex poisoned".to_string())?
        .clone()
        .ok_or_else(|| "STT model not ready — complete the setup wizard first".to_string())?;

    tauri::async_runtime::spawn(crate::audio::run_stt_loop(rx, client, sample_rate, channels, app_handle));

    Ok(())
}

/// Current mic energy level (0.0–1.0) for the waveform visualiser.
#[tauri::command]
pub fn get_audio_energy(energy: State<'_, SharedAudioEnergy>) -> f32 {
    use std::sync::atomic::Ordering;
    f32::from_bits(energy.load(Ordering::Relaxed))
}

#[tauri::command]
pub async fn stop_voice_input(
    voice_stop: State<'_, SharedVoiceStop>,
    energy: State<'_, SharedAudioEnergy>,
) -> CmdResult<()> {
    use std::sync::atomic::Ordering;
    if let Ok(mut g) = voice_stop.inner().lock() {
        if let Some(flag) = g.take() {
            flag.store(true, Ordering::Relaxed);
        }
    }
    // Reset energy to 0 when mic stops
    energy.store(0u32, Ordering::Relaxed);
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
    let binary = match crate::find_sidecar("llama-server") {
        Some(b) => b,
        None => return (false, "binary not found — use the Setup Wizard to download it".to_string()),
    };
    let dll_dir = binary.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(crate::binaries_dir);

    let priority_path = format!(
        "{};{}",
        dll_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(&binary)
            .arg("--version")
            .current_dir(&dll_dir)
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
    let url = crate::constants::VCREDIST_URL;
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

    // Security: verify SHA256 before executing the downloaded binary.
    // Pin to a specific VCRedist version + hash rather than blindly running whatever aka.ms serves.
    // Hash + URL are in constants.rs — update both together when pinning a new version.
    let file_bytes = tokio::fs::read(&dest).await.map_err(to_cmd_err)?;
    if !verify_sha256(&file_bytes, crate::constants::VCREDIST_SHA256) {
        let _ = tokio::fs::remove_file(&dest).await;
        return Err(format!("VCRedist integrity check failed — file removed for safety. \
                    Retry or install manually from {}", crate::constants::VCREDIST_URL));
    }

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
        Some(0) | Some(3010) => {
            // VCRedist updated the DLLs in System32 — copy them into binaries/ too.
            // This ensures our sidecars find the correct version in their own directory
            // (exe directory has highest DLL search priority) regardless of what else
            // is on the system PATH.
            copy_vcredist_dlls_to_binaries();
            Ok(())
        }
        code => Err(format!("installer exited with code {code:?}")),
    }
}

/// Copy the Visual C++ runtime DLLs from System32 into our binaries/ directory.
/// Called automatically after VCRedist install and can be triggered manually.
fn copy_vcredist_dlls_to_binaries() {
    let root = crate::binaries_dir();
    let system32 = std::path::Path::new("C:\\Windows\\System32");
    let dlls = [
        "VCRUNTIME140.dll", "VCRUNTIME140_1.dll",
        "MSVCP140.dll", "MSVCP140_2.dll", "CONCRT140.dll",
    ];
    // Copy into every sidecar subdirectory so each exe finds the correct version
    let targets: Vec<std::path::PathBuf> = [
        root.join("llama"),
        root.clone(),   // legacy flat layout fallback
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect();

    for dll in &dlls {
        let src = system32.join(dll);
        if src.exists() {
            for dir in &targets {
                let _ = std::fs::copy(&src, dir.join(dll));
            }
        }
    }
}

/// Diagnose what is actually running on the chat server port.
/// Dev-only — stripped from release builds.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn diagnose_chat_server(
    config: State<'_, SharedConfig>,
    chat_child: State<'_, SharedChatChild>,
) -> CmdResult<String> {
    let our_pid = {
        let guard = chat_child.lock().await;
        guard.as_ref().and_then(|c| c.id())
    };

    // Who owns the chat port right now, and what process is it?
    let chat_port = {
        let cfg = config.read().await;
        cfg.llama_port
    };
    let port_info = tokio::process::Command::new("powershell")
        .args(["-Command", &format!(
            "Get-NetTCPConnection -LocalPort {chat_port} -State Listen -ErrorAction SilentlyContinue | \
             Select-Object LocalPort,OwningProcess,@{{N='ProcessName';E={{(Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName}}}} | \
             Format-Table -AutoSize | Out-String"
        )])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "netstat failed".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let mut results = format!(
        "our chat child PID: {:?}\nchat port: {chat_port}\n\nPort {chat_port} owner (with process name):\n{}\n",
        our_pid, port_info
    );

    // GET endpoints
    for path in &["/health", "/props", "/v1/models", "/slots"] {
        let url = format!("http://{}:{chat_port}{path}", crate::SIDECAR_HOST);
        let resp = client.get(&url).send().await;
        match resp {
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                results.push_str(&format!("\nGET {} → {}\n{}\n", path, status, &body[..body.len().min(200)]));
            }
            Err(e) => results.push_str(&format!("\nGET {} → ERROR: {}\n", path, e)),
        }
    }
    // POST endpoints
    for (path, body) in &[
        ("/tokenize",           r#"{"content":"hello"}"#),
        ("/completion",         r#"{"prompt":"hello","n_predict":1}"#),
        ("/v1/chat/completions", r#"{"model":"llama-chat","messages":[{"role":"user","content":"hi"}]}"#),
    ] {
        let url = format!("http://{}:{chat_port}{path}", crate::SIDECAR_HOST);
        let resp = client.post(&url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await;
        match resp {
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                results.push_str(&format!("\nPOST {} → {}\n{}\n", path, status, &body[..body.len().min(200)]));
            }
            Err(e) => results.push_str(&format!("\nPOST {} → ERROR: {}\n", path, e)),
        }
    }
    // Also probe the embed server to compare route availability
    let embed_port = {
        let cfg = config.read().await;
        cfg.embed_port
    };
    results.push_str(&format!("\n\n── PORT {embed_port} (embed server) ──"));
    for (path, body, method) in &[
        ("/health",  "",                           "GET"),
        ("/props",   "",                           "GET"),
        ("/v1/models","",                          "GET"),
        ("/tokenize", r#"{"content":"hello"}"#,   "POST"),
        ("/v1/embeddings", r#"{"input":"hello","model":"nomic-embed-text"}"#, "POST"),
        ("/completion",    r#"{"prompt":"hello","n_predict":1}"#, "POST"),
    ] {
        let url = format!("http://{}:{embed_port}{path}", crate::SIDECAR_HOST);
        let resp = if *method == "GET" {
            client.get(&url).send().await
        } else {
            client.post(&url).header("Content-Type","application/json").body(body.to_string()).send().await
        };
        match resp {
            Ok(r) => {
                let status = r.status();
                let body_txt = r.text().await.unwrap_or_default();
                results.push_str(&format!("\n{method} {} → {}\n{}\n", path, status, &body_txt[..body_txt.len().min(150)]));
            }
            Err(e) => results.push_str(&format!("\n{method} {} → ERROR: {}\n", path, e)),
        }
    }

    Ok(results)
}

/// Open a new console window that runs llama-server --version directly.
/// Windows shows a GUI popup naming the exact DLL and function that is
/// missing before the process even starts — this is the definitive diagnostic.
/// Dev-only — stripped from release builds.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn open_llama_diagnostic() -> CmdResult<()> {
    let binary = crate::find_sidecar("llama-server")
        .ok_or_else(|| "llama-server not found — run: npm run setup".to_string())?;
    let dll_dir = binary.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(crate::binaries_dir);
    let binary_name = binary.file_name()
        .and_then(|n| n.to_str()).unwrap_or("llama-server.exe").to_string();

    // Write a batch file — avoids every cmd.exe inline quoting issue
    let bat = std::env::temp_dir().join("proactive_agent_diag.bat");
    let content = format!(
        "@echo off\r\ncd /d \"{dir}\"\r\necho.\r\necho Dir:  {dir}\r\necho Exe:  {name}\r\necho.\r\n{name} --version\r\necho.\r\nif %ERRORLEVEL% neq 0 (\r\n    echo FAILED  exit code: %ERRORLEVEL%\r\n    echo If a popup appeared, note the DLL name it mentions.\r\n) else (\r\n    echo OK  binary works!\r\n)\r\necho.\r\npause\r\n",
        dir  = dll_dir.display(),
        name = binary_name,
    );
    std::fs::write(&bat, content).map_err(to_cmd_err)?;

    // Spawn cmd.exe /K <bat> with CREATE_NEW_CONSOLE so it opens in its own window.
    // Using std::process::Command (not tokio) so we can set Windows creation flags.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        std::process::Command::new("cmd")
            .arg("/K")
            .arg(&bat)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(to_cmd_err)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS / Linux: open a terminal with the equivalent shell script
        let sh = std::env::temp_dir().join("proactive_agent_diag.sh");
        let sh_content = format!(
            "#!/bin/sh\ncd '{}'\necho 'Testing: {}'\n'{}' --version\necho 'Exit: '$?\nread -p 'Press Enter to close'\n",
            bin_dir.display(), binary_name,
            bin_dir.join(&binary_name).display()
        );
        std::fs::write(&sh, &sh_content).map_err(to_cmd_err)?;
        let _ = std::process::Command::new("chmod").args(["+x", sh.to_str().unwrap_or("")]).status();
        std::process::Command::new("open")
            .args(["-a", "Terminal", sh.to_str().unwrap_or("")])
            .spawn()
            .map_err(to_cmd_err)?;
    }

    Ok(())
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

/// Full reset: wipes episodic memory, semantic facts, and recent turns window.
/// The model starts completely fresh with no knowledge of past conversations.
#[tauri::command]
pub async fn reset_chat(orchestrator: State<'_, SharedOrchestrator>) -> CmdResult<()> {
    let mut lock = orchestrator.lock().await;
    if let Some(ref mut orch) = *lock {
        orch.reset_memory().await.map_err(to_cmd_err)?;
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
    // Guard against runaway input — 32 KB is generous for any real message
    const MAX_MSG_BYTES: usize = 32 * 1024;
    if message.len() > MAX_MSG_BYTES {
        return Err(format!("message too long ({} bytes, max {MAX_MSG_BYTES})", message.len()));
    }

    let mut lock = orchestrator.lock().await;
    let orch = lock.as_mut().ok_or("Orchestrator not yet initialised")?;

    let (response, deferred) =
        orch.send_message(message, &app_handle).await.map_err(to_cmd_err)?;

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
    process_pids: State<'_, SharedProcessPids>,
    app_handle: tauri::AppHandle,
    model_path: String,
) -> CmdResult<()> {
    // Security: validate path before accepting it
    let path = std::path::Path::new(&model_path);
    if !path.is_absolute() {
        return Err("model_path must be an absolute path".to_string());
    }
    if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
        return Err("model_path must point to a .gguf file".to_string());
    }
    if !path.is_file() {
        return Err("model file does not exist".to_string());
    }

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
        process_pids.inner().clone(),
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

// ── Model generation parameters ──────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
pub struct GenSettings {
    pub temperature: f32,
    pub top_p: f32,
    pub context_window_tokens: usize,
}

#[tauri::command]
pub async fn get_gen_settings(config: State<'_, SharedConfig>) -> CmdResult<GenSettings> {
    let cfg = config.read().await;
    Ok(GenSettings {
        temperature: cfg.temperature,
        top_p: cfg.top_p,
        context_window_tokens: cfg.context_window_tokens,
    })
}

#[tauri::command]
pub async fn set_gen_settings(
    config: State<'_, SharedConfig>,
    app_handle: tauri::AppHandle,
    settings: GenSettings,
) -> CmdResult<()> {
    let mut cfg = config.write().await;
    cfg.temperature = settings.temperature.clamp(0.0, 2.0);
    cfg.top_p = settings.top_p.clamp(0.0, 1.0);
    cfg.context_window_tokens = settings.context_window_tokens.clamp(512, 131_072);
    if let Ok(config_path) = app_handle.path().app_config_dir() {
        let _ = cfg.save(&config_path.join("config.json"));
    }
    Ok(())
}

/// Inject a fake <defer> response to test the proactivity pipeline end-to-end
/// without needing the model to actually emit the tag.
/// Dev-only — stripped from release builds.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn test_defer(
    scheduler: State<'_, SharedScheduler>,
    app_handle: tauri::AppHandle,
    message: String,
    after_minutes: Option<i64>,
) -> CmdResult<String> {
    use crate::monitor::DeferredMessage;
    use chrono::Utc;
    use uuid::Uuid;

    let mins = after_minutes.unwrap_or(1);
    let msg = DeferredMessage {
        id: Uuid::new_v4().to_string(),
        message: message.clone(),
        trigger: "manual_test".to_string(),
        fire_at: Utc::now() + chrono::Duration::minutes(mins),
    };
    let id = msg.id.clone();
    scheduler.lock().await.add(msg);
    // If after_minutes == 0, fire immediately for testing
    if mins == 0 {
        if let Some(m) = scheduler.lock().await.fire_now(&id) {
            app_handle.emit("proactive_message", &m).map_err(to_cmd_err)?;
            return Ok(format!("fired immediately: {}", m.message));
        }
    }
    Ok(format!("scheduled in {mins} min — use 'Fire Now' in Scheduler panel or set after_minutes=0"))
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

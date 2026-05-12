/// Application-wide constants.
///
/// Rule: no literal URLs, model filenames, timeout values, or magic numbers
/// outside this file (or their owning module if strictly local).
/// This makes version bumps, URL changes, and timing adjustments a one-line edit.

// ── Sidecar networking ────────────────────────────────────────────────────────

/// Loopback address for all sidecar HTTP servers. Explicitly IPv4 —
/// "localhost" can resolve to ::1 (IPv6) on some systems.
/// Defined here AND re-exported from lib.rs as `crate::SIDECAR_HOST`.
pub const SIDECAR_HOST: &str = "127.0.0.1";

// ── Audio ─────────────────────────────────────────────────────────────────────

/// ONNX Runtime CPU-only DLL for ort rc.12.
/// Microsoft's official CPU package — no DirectML/GPU dependencies.
/// ORT 1.19.2 implements API version 18+ which satisfies ort rc.12's minimum of 17.
pub const ORT_CPU_DLL_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.19.2/onnxruntime-win-x64-1.19.2.zip";

/// Target sample rate for STT input. Parakeet TDT (and Whisper) were trained
/// on 16 kHz mono audio — sending the native device rate and relying on the
/// server to resample gives lower quality than doing it ourselves with rubato.
pub const STT_SAMPLE_RATE: u32 = 16_000;

// ── Model filenames ───────────────────────────────────────────────────────────

/// Embed model filename inside `models_dir`. Fixed — changing this requires
/// re-embedding all stored vectors (schema encodes 768-dim).
pub const EMBED_MODEL_FILE: &str = "nomic-embed-text-v1.5.Q8_0.gguf";

/// Embed model alias sent to llama-server via --alias. Must match the
/// string used in embedding HTTP requests.
pub const EMBED_MODEL_ALIAS: &str = "nomic-embed-text";

/// Piper TTS voice model filename inside `models/tts/`.
pub const TTS_MODEL_FILE: &str = "en_US-lessac-medium.onnx";

/// Parakeet STT ONNX model filename inside `binaries/parakeet/models/`.
pub const STT_MODEL_FILE: &str = "parakeet-tdt-0.6b-v3.onnx";

/// Parakeet STT vocabulary/tokens file alongside the ONNX model.
pub const STT_TOKENS_FILE: &str = "parakeet-tdt-0.6b-v3-tokens.txt";

// ── Download URLs ─────────────────────────────────────────────────────────────

/// nomic-embed-text GGUF download from HuggingFace.
/// Update EMBED_MODEL_FILE when changing this URL.
pub const EMBED_MODEL_URL: &str =
    "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf";

/// Parakeet TDT ONNX model download from HuggingFace.
pub const STT_MODEL_URL: &str =
    "https://huggingface.co/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai/resolve/main/parakeet-tdt-0.6b-v3.onnx";

/// Parakeet vocabulary file download.
pub const STT_TOKENS_URL: &str =
    "https://huggingface.co/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai/resolve/main/parakeet-tdt-0.6b-v3-tokens.txt";

/// Visual C++ Redistributable 2022 x64 silent installer from Microsoft.
/// TODO: pin to a specific versioned URL and update VCREDIST_SHA256 when bumping.
pub const VCREDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

/// SHA256 of the VCRedist installer. Intentionally a placeholder —
/// replace with the real hash before shipping.
/// Source: https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist
pub const VCREDIST_SHA256: &str =
    "c760c594b9f5e8cb76be9bba6e4a38b0dd13e3bd5a8cf4d05d4e7a4b5e1b2c3d4";

// ── Timing ────────────────────────────────────────────────────────────────────

/// Milliseconds to wait after killing the old chat server before starting
/// a new one (lets the OS release the port and file handles).
pub const CHAT_SERVER_RESTART_DELAY_MS: u64 = 600;

/// Milliseconds to wait after startup before emitting system_deps_checked,
/// giving the frontend time to mount its event listener.
pub const DEPS_CHECK_STARTUP_DELAY_MS: u64 = 800;

/// Seconds between semantic distillation runs.
pub const DISTILLATION_INTERVAL_SECS: u64 = 600; // 10 minutes

/// Seconds to wait after startup before the first distillation run,
/// giving the orchestrator time to initialise.
pub const DISTILLATION_STARTUP_DELAY_SECS: u64 = 60;

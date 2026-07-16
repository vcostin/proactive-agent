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

/// Target sample rate for STT input. Parakeet TDT expects 16 kHz mono —
/// the capture path negotiates device format then converts to this contract.
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

/// Directory under binaries/ holding Host STT model artifacts.
pub const STT_MODEL_REL_DIR: &str = "parakeet/models";

/// Parakeet TDT encoder ONNX (int8) inside `binaries/parakeet/models/`.
pub const STT_ENCODER_FILE: &str = "encoder-model.int8.onnx";

/// Parakeet TDT decoder-joint ONNX (int8) inside `binaries/parakeet/models/`.
pub const STT_DECODER_FILE: &str = "decoder_joint-model.int8.onnx";

/// Parakeet TDT vocabulary file alongside the ONNX models.
pub const STT_VOCAB_FILE: &str = "vocab.txt";

/// Directory under binaries/ for the app-managed ONNX Runtime shared library.
pub const ORT_LIB_REL_DIR: &str = "ort";

/// Documented ORT shared-library basename (verify uses SharedLibPresent).
#[cfg(target_os = "windows")]
pub const ORT_LIB_FILENAME: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
pub const ORT_LIB_FILENAME: &str = "libonnxruntime.dylib";
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub const ORT_LIB_FILENAME: &str = "libonnxruntime.so";

// ── Download URLs ─────────────────────────────────────────────────────────────

/// nomic-embed-text GGUF download from HuggingFace.
/// Update EMBED_MODEL_FILE when changing this URL.
pub const EMBED_MODEL_URL: &str =
    "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf";

/// Parakeet TDT encoder download (istupakov/parakeet-tdt-0.6b-v3-onnx).
pub const STT_ENCODER_URL: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.int8.onnx";

/// Parakeet TDT decoder-joint download.
pub const STT_DECODER_URL: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/decoder_joint-model.int8.onnx";

/// Parakeet vocabulary download.
pub const STT_VOCAB_URL: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt";

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

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

/// ONNX Runtime CPU-only DLL — from the NuGet CPU package, NOT the GitHub zip.
/// The GitHub win-x64 zip still includes DirectML which initializes GPU hardware
/// during DllMain and hangs on some systems. The NuGet Microsoft.ML.OnnxRuntime
/// package is built WITHOUT DirectML/GPU providers — truly CPU only.
pub const ORT_CPU_DLL_URL: &str =
    "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.19.2/microsoft.ml.onnxruntime.1.19.2.nupkg";
/// Path inside the NuGet package (which is a zip) to the CPU-only DLL.
pub const ORT_CPU_DLL_PATH_IN_PKG: &str = "runtimes/win-x64/native/onnxruntime.dll";
pub const ORT_CPU_SHARED_PATH_IN_PKG: &str = "runtimes/win-x64/native/onnxruntime_providers_shared.dll";

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

// ── Download URLs — pinned releases ──────────────────────────────────────────
// Pinned to specific versions to avoid GitHub API rate limits (60 req/hr).
// Update these constants when upgrading a component.

/// llama.cpp release tag — CPU server + Vulkan DLLs.
pub const LLAMA_VERSION: &str = "b9095";
pub const LLAMA_CPU_URL_WIN: &str =
    "https://github.com/ggerganov/llama.cpp/releases/download/b9095/llama-b9095-bin-win-cpu-x64.zip";
pub const LLAMA_VULKAN_URL_WIN: &str =
    "https://github.com/ggerganov/llama.cpp/releases/download/b9095/llama-b9095-bin-win-vulkan-x64.zip";
pub const LLAMA_CPU_URL_MAC_ARM: &str =
    "https://github.com/ggerganov/llama.cpp/releases/download/b9095/llama-b9095-bin-macos-arm64.zip";
pub const LLAMA_CPU_URL_LINUX: &str =
    "https://github.com/ggerganov/llama.cpp/releases/download/b9095/llama-b9095-bin-ubuntu-x64.zip";

/// Piper TTS — includes onnxruntime.dll, DLLs, espeak-ng-data.
pub const PIPER_URL_WIN: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
pub const PIPER_URL_MAC_ARM: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_aarch64.tar.gz";
pub const PIPER_URL_LINUX: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz";

/// nomic-embed-text GGUF download from HuggingFace.
pub const EMBED_MODEL_URL: &str =
    "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf";

/// Parakeet TDT ONNX model download from HuggingFace.
pub const STT_MODEL_URL: &str =
    "https://huggingface.co/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai/resolve/main/parakeet-tdt-0.6b-v3.onnx";

/// Parakeet vocabulary file download.
pub const STT_TOKENS_URL: &str =
    "https://huggingface.co/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai/resolve/main/parakeet-tdt-0.6b-v3-tokens.txt";

/// Visual C++ Redistributable 2022 x64 silent installer from Microsoft.
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

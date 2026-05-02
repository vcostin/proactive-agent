# Setup Guide

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable | `rustup update stable` |
| Node | 18+ | LTS recommended |
| protoc | any | `winget install Google.Protobuf` (Windows) / `brew install protobuf` (macOS) |
| Python | 3.10+ | Only needed for Kokoro TTS |

---

## First-time setup

### 1. Cargo config (required — lance needs protoc)

Copy and fill in your `protoc` path:

```
cp src-tauri/.cargo/config.toml.example src-tauri/.cargo/config.toml
# Edit config.toml — set PROTOC to the full path of protoc.exe / protoc
```

### 2. Download sidecar binaries + models

**Windows (AMD GPU — Vulkan backend):**
```powershell
.\scripts\fetch-sidecars-windows.ps1
```

This downloads:
- `llama-server.exe` (Vulkan build from latest llama.cpp release)
- `whisper-server.exe` (from latest whisper.cpp release)
- `ggml-base.en.bin` (~142 MB — Whisper base English model)
- `nomic-embed-text-v1.5.Q8_0.gguf` (~274 MB — embedding model)

**macOS (Apple Silicon — Metal backend):**
```bash
chmod +x scripts/fetch-sidecars-macos.sh
./scripts/fetch-sidecars-macos.sh
```

### 3. Add a chat model

Drop any `.gguf` chat model into `models/`. Recommended for your hardware
(16 GB VRAM):

| Model | Size | Quality |
|-------|------|---------|
| Qwen2.5-14B-Instruct-Q8_0.gguf | ~15 GB | excellent |
| Mistral-7B-Instruct-v0.3.Q8_0.gguf | ~8 GB | fast |
| Llama-3.1-8B-Instruct-Q8_0.gguf | ~9 GB | solid |

Good sources: [Hugging Face](https://huggingface.co/models?library=gguf),
[LM Studio model library](https://lmstudio.ai/models).

### 4. Set up Kokoro TTS (optional)

Kokoro TTS requires Python. Skip this if you don't need voice output.

**Option A — run Python server during dev (simplest):**
```bash
pip install kokoro-onnx soundfile fastapi uvicorn
python scripts/kokoro_server.py --port 8083
```

**Option B — compile to .exe (for distribution):**
```powershell
.\scripts\build-kokoro-exe.ps1
# Outputs: binaries/kokoro-server-x86_64-pc-windows-msvc.exe
```

### 5. Install frontend dependencies

```bash
npm install
```

### 6. Run

```bash
npm run tauri dev
```

The app will:
1. Start all four sidecar processes in the background
2. Open LanceDB in `data/memory/`
3. Load the UI — chat tab ready within a few seconds

---

## Selecting a chat model

1. Click the **models** tab
2. Your `.gguf` files appear automatically
3. Click **load** — the model is swapped live without restarting

The first time you load a model, llama-server will take 10–30 seconds to
load it into VRAM. The sidecar health panel turns green when it's ready.

---

## Directory layout after setup

```
proactive-agent/
├── binaries/
│   ├── llama-server-x86_64-pc-windows-msvc.exe   ← from fetch script
│   ├── whisper-server-x86_64-pc-windows-msvc.exe ← from fetch script
│   ├── kokoro-server-x86_64-pc-windows-msvc.exe  ← from build-kokoro-exe.ps1
│   └── *.dll                                      ← bundled with llama/whisper
├── models/
│   ├── your-chat-model.gguf                       ← you provide this
│   ├── nomic-embed-text-v1.5.Q8_0.gguf           ← from fetch script
│   └── ggml-base.en.bin                           ← from fetch script
└── data/
    └── memory/                                    ← created on first run
```

---

## Ports

| Sidecar | Default port | Purpose |
|---------|-------------|---------|
| llama-server (chat) | 8080 | LLM inference |
| llama-server (embed) | 8081 | nomic-embed-text embeddings |
| whisper-server | 8082 | Speech-to-text |
| kokoro-server | 8083 | Text-to-speech |

All configurable in `AppConfig` defaults (`src-tauri/src/config.rs`).

---

## Troubleshooting

**Sidecar shows red in debug panel** — binary missing or crashed.
Check the Event Log panel for `[ADAPTER]` messages.

**`cargo check` fails with "protoc not found"** — see step 1.

**LanceDB fails to open** — `data/memory/` may be corrupted from an
interrupted write. Delete the `data/` directory and restart.

**llama-server exits immediately** — model file path wrong, or VRAM
insufficient for the loaded quant. Check Event Log.

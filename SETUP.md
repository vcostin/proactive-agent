# Setup Guide

Preferred toolchain: **Deno 2+**. Node 20+ still works via `package.json` scripts.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable 1.77+ | [rustup](https://rustup.rs/) |
| Deno | 2+ | Preferred — [deno.com](https://deno.com/) |
| Node | 20+ | Optional if using Deno |
| protoc | any | Needed by LanceDB (`winget` / `pacman` / `brew`) |
| ffmpeg | any | Linux Parakeet STT only (`pacman -S ffmpeg`) |
| uv | any | Linux Parakeet STT only ([astral.sh/uv](https://docs.astral.sh/uv/)) |
| WebView | OS-specific | WebKitGTK 4.1 (Linux), WebView2 (Windows) — see [Tauri prerequisites](https://tauri.app/start/prerequisites/) |

---

## First-time setup

### 1. Cargo config (required — LanceDB needs protoc)

```bash
cp src-tauri/.cargo/config.toml.example src-tauri/.cargo/config.toml
# Edit config.toml — set PROTOC to the full path of protoc
```

### 2. Install JS deps + download sidecars / models

```bash
deno install
deno task setup              # OS-detecting
# or explicitly:
#   deno task setup:linux
#   deno task setup:mac
#   deno task setup:windows
```

`deno task setup` (Linux) downloads:

- `llama-server` (Vulkan-capable build + shared libs)
- `piper` + voice model + `espeak-ng-data`
- `nomic-embed-text-v1.5.Q8_0.gguf`
- Parakeet STT managed installer → launcher at
  `binaries/parakeet/parakeet-server-x86_64-unknown-linux-gnu`
  (venv + models under `.cache/parakeet-tdt/`)

Windows setup fetches llama + piper + models; Parakeet still needs a frozen sidecar
when you want mic input there.

Node equivalent: `npm install && npm run setup`.

### 3. Add a chat model

Drop any `.gguf` chat model into `models/`. Examples for ~16 GB VRAM:

| Model | Size | Quality |
|-------|------|---------|
| Qwen2.5-14B-Instruct-Q8_0.gguf | ~15 GB | excellent |
| Qwen3-8B Q5_K_M (or similar) | ~5–6 GB | good, lighter |
| Mistral-7B-Instruct-v0.3.Q8_0.gguf | ~8 GB | fast |

Sources: [Hugging Face](https://huggingface.co/models?library=gguf),
[LM Studio model library](https://lmstudio.ai/models).

### 4. Run

```bash
deno task tauri dev          # or: npm run tauri -- dev
```

On launch the app:

1. Spawns llama (embed), Parakeet STT (if the sidecar/launcher is present), and loads Piper on demand
2. Opens LanceDB under `data/memory/` (dev) or the OS app-data memory path (release)
3. Opens the UI — pick/load a chat model on the **Models** tab if needed

Parakeet on Linux starts with the app (no second terminal). First cold start can take a few
seconds while the ONNX model loads; the mic waits briefly for `/healthz` before failing.

Manual STT foreground (debug): `deno task parakeet:linux`.

---

## Selecting a chat model

1. Open the **models** tab
2. Your `.gguf` files appear automatically
3. Click **load** — hot-swapped without restarting the app

First load can take 10–30 seconds while llama-server maps the GGUF into VRAM.
The Sidecar Health panel turns green when chat inference is ready.

---

## Directory layout after setup

```
proactive-agent/
├── binaries/
│   ├── llama/            llama-server + Vulkan / DLL deps
│   ├── parakeet/         launcher (Linux) or frozen exe (Windows) + models/
│   └── piper/            piper + espeak-ng-data + ORT libs
├── .cache/parakeet-tdt/  Linux only — Python venv + HF cache (from setup)
├── models/
│   ├── your-chat-model.gguf
│   ├── nomic-embed-text-v1.5.Q8_0.gguf
│   └── tts/              Piper voice onnx + json
└── data/
    └── memory/           LanceDB (created on first run in dev)
```

---

## Ports

| Sidecar | Default port | Purpose |
|---------|-------------|---------|
| llama-server (chat) | 18080 | LLM inference |
| llama-server (embed) | 18081 | nomic-embed-text embeddings |
| Parakeet STT | 5092 | Speech-to-text (HTTP; eliminated by future `ort` migration) |

Piper is **not** a long-running server — one subprocess per utterance.

Defaults live in `AppConfig` (`src-tauri/src/config.rs`). Override via the config file.

Port range **18080+** avoids LM Studio and other tools on `8080`.

---

## Troubleshooting

**Parakeet / STT red or mic banner about port 5092** — Launcher missing or crash on start.
On Linux re-run `deno task setup` (or `deno task parakeet:linux`). In **Debug → Event Log**,
look for `[ADAPTER] Parakeet STT …`. The Linux launcher is a small shell script; it must
be executable and discoverable by `find_sidecar`.

**Sidecar shows red** — Binary missing or exited. Check Event Log `[ADAPTER]` lines.

**`cargo check` fails with "protoc not found"** — Step 1.

**LanceDB fails to open** — Delete the `data/` directory (dev) and restart.

**llama-server exits immediately** — Wrong model path or insufficient VRAM. Check Event Log.

**Linux STT works manually but not from the app** — Restart `deno task tauri` after
setup so it can spawn the refreshed launcher; ensure nothing else is bound to `:5092`.

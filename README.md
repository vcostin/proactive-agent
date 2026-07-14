# Proactive Agent

A fully local, voice-enabled AI assistant with persistent memory and proactive behaviour.
Runs on your machine — no cloud, no telemetry, no API keys required.

---

## What it does

- **Talks and listens** — speak to it with a mic, hear responses aloud
- **Remembers you** — stores conversation history in a local vector database; retrieves relevant memories on every turn
- **Thinks ahead** — the LLM can schedule follow-up messages to itself, which fire later without you doing anything ("proactive" mode)
- **Stays local** — inference, speech recognition, and text-to-speech all run on your GPU/CPU

---

## System requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| OS | Linux x86_64 / Windows 10 x64 | Arch / Windows 11 |
| RAM | 16 GB | 32 GB+ |
| GPU | Any (CPU fallback) | Vulkan-capable with 8 GB+ VRAM |
| Disk | 15 GB free | 30 GB free |
| JS toolchain | **Deno 2+** (preferred) or Node 20+ | Deno |

> Voice input works on **Linux** (managed Parakeet launcher, auto-started with the app) and **Windows** (frozen sidecar when present). Long-term STT target is in-process `ort` — see `STT_ORT_MIGRATION.md`.

---

## Quick start (Linux / Deno)

```bash
deno install                 # installs npm deps into node_modules/
deno task setup              # llama, piper, embed/TTS models, Parakeet launcher
# drop a chat .gguf into models/
deno task tauri dev          # Parakeet STT starts with the app on :5092
```

Node is still supported: `npm install && npm run setup && npm run tauri -- dev`.

---

## Quick start (from installer)

1. Run `proactive-agent_0.1.0_x64_en-US.msi` or `proactive-agent_0.1.0_x64-setup.exe`
2. Launch **Proactive Agent** from the Start Menu
3. The setup wizard walks you through downloading required models (~4–8 GB depending on choice)
4. Start chatting

---

## Developer setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.77+)
- [Deno](https://deno.com/) 2+ **or** [Node.js](https://nodejs.org/) 20+
- [Tauri CLI prerequisites](https://tauri.app/start/prerequisites/) (WebKitGTK 4.1 on Linux, WebView2 on Windows)

### Install

```bash
git clone <repo>
cd proactive-agent
deno install                 # or: npm install
deno task setup              # OS-detecting; or setup:linux / setup:mac / setup:windows
```

### Run in dev mode

```bash
deno task tauri dev          # or: npm run tauri -- dev
```

The app opens with the setup wizard on first run. Pick a model, let it download, start chatting.

### Build installer

```bash
deno task tauri build
# Linux → src-tauri/target/release/bundle/...
# Windows → msi / nsis under the same path
```

---

## Voice

| Feature | Default | Toggle |
|---------|---------|--------|
| Mic input (STT) | Off | 🎤 button in chat header |
| Voice output (TTS) | Off | 🔊 button in chat header |

**STT:** Speak → silence → transcribed and sent automatically. Requires Parakeet on
`:5092` (started automatically on Linux; frozen sidecar or manual start on Windows).
If STT is still warming up, the mic waits briefly before failing with a visible error.

**TTS:** Every assistant response is read aloud. Markdown, code blocks, and URLs are
stripped before synthesis so they don't get read literally.

Manual Linux Parakeet (debug only): `deno task parakeet:linux`.

---

## Memory

The agent maintains two memory layers:

- **Episodic** — every conversation turn, embedded and stored in LanceDB. Semantically
  similar past turns are retrieved and injected into context on each new message.
- **Semantic** — long-term facts distilled from episodic memory by a background task
  that runs every 10 minutes. Stores persistent information like your preferences,
  ongoing projects, and recurring topics.

To reset memory: type `RESET` in the chat input and confirm. This wipes both layers
and clears recent turns. The app config and models are not affected.

---

## Proactivity

The LLM can append a `<defer>` tag to any response:

```
<defer>{"message": "Did you finish that report?", "after_minutes": 60, "trigger": "follow_up"}</defer>
```

The Rust scheduler stores this, fires the message after the specified delay, and delivers
it to the chat window as if the agent just thought of it. The tag is always stripped
from the visible response — you only see the follow-up when it fires.

Use the **Scheduler** panel in the debug tab to inspect pending deferred messages
or trigger them immediately for testing.

---

## Debug panel

Open the **Debug** tab for live system state:

| Panel | Shows |
|-------|-------|
| Sidecar Health | Green/red dot per process, port, latency |
| Memory Stats | Episodic + semantic row counts, last write/distillation time |
| Memory Browser | Scrollable list of recent episodic entries |
| Audio State | VAD status, mic energy level, last STT/TTS latencies |
| Scheduler | Pending deferred messages, "Fire Now" button |
| Context Inspector | Exact context sent on last LLM call, token counts per section |
| Event Log | Live stream of internal events, filterable by component tag |

---

## Config file

| OS | Path |
|----|------|
| Windows | `%APPDATA%\com.proactive.agent\config.json` |
| Linux | `~/.config/com.proactive.agent/config.json` (or XDG config) |
| Dev | under the project `data/` tree |

Delete this file to reset all settings to defaults (model path, ports, persona prompt,
temperature, context window, etc.). The app recreates it on next launch.

---

## Model hot-swap

Open the **Models** tab, click **Load** on any `.gguf` file in the models directory.
The chat server restarts in the background — no app restart needed. The embedding model
(`nomic-embed-text`) is fixed and never swapped; it runs on a separate server instance
so vector space compatibility is preserved across chat model changes.

---

## macOS

Script exists at `scripts/fetch-sidecars-macos.sh` but is untested.
STT on macOS waits on the `ort` migration (no frozen sidecar yet) — see `ROADMAP.md`.

## Platform support

| | Windows | Linux | macOS |
|--|---------|-------|-------|
| Setup | `deno task setup` / `setup:windows` | `deno task setup` | `deno task setup` / `setup:mac` |
| GPU | Vulkan + VCRedist check | Vulkan | Metal (no Vulkan row) |
| STT (Parakeet) | Frozen sidecar when present | Auto-started with app (managed Python launcher) | Rebuild needed |
| TTS (Piper) | Yes | Yes | Yes |

System requirements adapt per OS: VCRedist is Windows-only; llama-server is probed on all three.
On Linux, `deno task setup` installs a Parakeet launcher under `binaries/parakeet/`; the app
spawns it on startup like llama embed (no separate terminal).

---

## Project layout

```
proactive-agent/
├── src/                  React frontend (TypeScript)
│   ├── components/
│   │   ├── chat/         ChatWindow, WaveformVisualizer
│   │   ├── debug/        DebugPanel and all sub-panels
│   │   ├── models/       ModelPanel, ModelList, ModelSelector
│   │   └── setup/        SetupWizard, SystemRequirements, RequirementsBanner
│   └── hooks/            useChat, useLlamaReady, useAudioEnergy
├── src-tauri/            Rust backend (Tauri 2)
│   └── src/
│       ├── orchestrator/ Conversation loop, context assembly, scheduler
│       ├── memory/       Episodic store, semantic store, embedding service
│       ├── audio/        Mic capture, STT client, TTS client
│       ├── commands.rs   All Tauri invoke endpoints
│       ├── monitor.rs    Health polling, debug event emitter
│       └── config.rs     Runtime config, persisted to AppData
├── binaries/             Sidecars (gitignored, from `deno task setup`)
│   ├── llama/            llama-server + Vulkan libs
│   ├── parakeet/         Windows: frozen exe · Linux: shell launcher (~600 B)
│   └── piper/            Piper TTS + espeak-ng-data
├── .cache/parakeet-tdt/  Linux Parakeet Python env (gitignored, from setup)
├── models/               GGUF model files (gitignored)
│   ├── *.gguf            Chat models (downloaded by user)
│   ├── nomic-embed-text-v1.5.Q8_0.gguf
│   └── tts/              en_US-lessac-medium.onnx + .json
├── scripts/
│   ├── setup.ts / setup.sh           # OS dispatcher
│   ├── run-frontend.ts / .sh         # Prefer Deno, fall back to npm
│   ├── run-parakeet-linux.sh         # Install/run Linux STT (+ --install)
│   ├── fetch-sidecars-linux.sh
│   ├── fetch-sidecars-macos.sh
│   └── fetch-sidecars-windows.ps1
├── deno.json             Deno tasks (preferred JS toolchain)
└── package.json          npm-compatible scripts (still supported)
```

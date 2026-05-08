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
| OS | Windows 10 x64 | Windows 11 x64 |
| RAM | 16 GB | 32 GB+ |
| GPU | Any (CPU fallback) | Vulkan-capable with 8 GB+ VRAM |
| Disk | 15 GB free | 30 GB free |
| Runtime | VCRedist 2015–2022 | (auto-installed on first run) |

> macOS (Apple Silicon) is supported in principle — see [macOS setup](#macos) below.

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
- [Node.js](https://nodejs.org/) 20+
- [Tauri CLI prerequisites](https://tauri.app/start/prerequisites/) (WebView2 on Windows)

### Install

```bash
git clone <repo>
cd proactive-ai
npm install
npm run setup          # downloads llama-server, piper binaries + DLLs into binaries/
```

### Run in dev mode

```bash
npm run tauri dev
```

The app opens with the setup wizard on first run. Pick a model, let it download, start chatting.

### Build installer

```bash
npm run tauri build
# → src-tauri/target/release/bundle/msi/proactive-agent_0.1.0_x64_en-US.msi
# → src-tauri/target/release/bundle/nsis/proactive-agent_0.1.0_x64-setup.exe
```

---

## Voice

| Feature | Default | Toggle |
|---------|---------|--------|
| Mic input (STT) | Off | 🎤 button in chat header |
| Voice output (TTS) | Off | 🔊 button in chat header |

**STT:** Speak → 800 ms of silence → transcribed and sent automatically.

**TTS:** Every assistant response is read aloud. Markdown, code blocks, and URLs are
stripped before synthesis so they don't get read literally.

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

`%APPDATA%\com.proactive.agent\config.json`

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
Piper has an `aarch64` build; Parakeet STT needs a macOS PyInstaller rebuild.
See `ROADMAP.md` for current status.

---

## Project layout

```
proactive-ai/
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
├── binaries/             Sidecar executables (gitignored, populated by npm run setup)
│   ├── llama/            llama-server + Vulkan DLLs
│   ├── parakeet/         Parakeet STT server (PyInstaller frozen)
│   └── piper/            Piper TTS + espeak-ng-data
├── models/               GGUF model files (gitignored)
│   ├── *.gguf            Chat models (downloaded by user)
│   ├── nomic-embed-text-v1.5.Q8_0.gguf
│   └── tts/              en_US-lessac-medium.onnx + .json
├── scripts/
│   ├── fetch-sidecars-windows.ps1
│   └── fetch-sidecars-macos.sh
├── ARCHITECTURE.md       Technical architecture and component alternatives
├── ROADMAP.md            What's done, what's remaining, what needs a decision
└── WORK_LOG.md           Per-session implementation notes and bug history
```

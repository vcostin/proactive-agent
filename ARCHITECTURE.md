# Proactive Local LLM Agent — Architecture & Skeleton Prompt

## Purpose

Living architecture document for a **proactive, voice-enabled, memory-augmented local LLM agent** built with **Tauri 2**, **Rust**, and **React**. The agent supports hot-swappable LLM backends, persistent vector memory, and a voice pipeline. Implementation is delivered in six committed phases — see the Implementation Phases section.

---

## Context You Must Internalize

You are generating the foundational skeleton of a desktop application. The developer is an experienced full-stack engineer with a Rust/Deno/React/Tauri 2 background. Do not over-explain basics. Be direct, opinionated, and make explicit architectural decisions rather than listing options.

The target machines are:
- **Windows** (primary): AMD discrete GPU with 16GB VRAM, 64GB DDR5 RAM — use **llama.cpp Vulkan backend** for inference
- **macOS M1 Max** (secondary): Metal backend via Ollama or llama.cpp

Both platforms must be supported from a single codebase.

---

## Known Implementation Risks

These are not open decisions — they are resolved. Noted here so they are not re-litigated.

**LanceDB Rust crate API stability**
The `lancedb` crate is the underlying core of the Python/JS library but its public Rust API has had breaking changes between minor versions. Pin the exact version in `Cargo.toml` and do not `cargo update` without verifying the async API against current docs. The `connect` → `open_table` → `merge_insert` surface is the one most likely to shift.

**Kokoro TTS streaming**
Kokoro is primarily batch inference — "stream as it generates" cannot be taken off the shelf. Implementation: split synthesis into sentence-sized chunks in the sidecar wrapper, open a PCM playback stream with cpal before the first chunk arrives, and feed chunks as they complete. Do not wait for full audio before starting playback.

**`<defer>` tag parsing**
Use a lenient regex (`(?s)<defer>(.*?)</defer>`) to extract the JSON payload — not a strict XML parser. Models emit malformed tags (unclosed, extra whitespace, escaped quotes). If the inner JSON is unparseable, log the failure via `debug_event` and discard the defer rather than erroring the whole response.

---

## Hard Constraints

- **Tauri 2** shell — no Electron, no web server
- **Rust** for all backend logic: orchestration, memory pipeline, audio pipeline, scheduler
- **React** frontend — chat UI, model selector, waveform visualizer
- **LanceDB** (Rust crate) as the embedded vector store — no external DB server
- **llama.cpp** as the primary inference sidecar (Vulkan on Windows, Metal on macOS) — expose OpenAI-compatible REST on `127.0.0.1:8080`
- **whisper.cpp** as the STT sidecar
- **Kokoro TTS** as the TTS sidecar
- **nomic-embed-text** via llama.cpp as the dedicated embedding model — this model slot is **locked and never swapped**, even when the chat model changes
- The entire stack runs locally — no cloud calls, no telemetry

---

## Core Architectural Principle

The **LLM chat model is a replaceable component**. The vector memory, audio pipeline, and persona are infrastructure that persists across model swaps. This must be encoded as a Rust trait:

```rust
#[async_trait]
pub trait ModelAdapter: Send + Sync {
    async fn complete(&self, context: AssembledContext) -> Result<ModelResponse>;
    fn model_id(&self) -> &str;
}
```

Embeddings are **not** routed through `ModelAdapter`. They go through a separate, fixed `EmbeddingService` that always uses `nomic-embed-text`. This prevents vector space incompatibility when the chat model is swapped.

---

## Memory Architecture

Two distinct memory layers, stored in the same LanceDB instance but in **separate collections**:

### 1. Episodic Memory (conversation turns)
- Raw turns are **not stored directly**
- Each turn goes through a processing step before storage:
  - Extract entities, intents, key facts
  - Attach metadata: `user_id`, `timestamp`, `session_id`, `importance_score`
  - Summarize if turn is low-signal (e.g. "ok", "thanks")
- Embedded with `nomic-embed-text` → stored in `episodic` collection

### 2. Semantic Memory (extracted long-term facts)
- Distilled from episodic memory by a background task
- Stores persistent facts about the user: preferences, ongoing projects, recurring topics
- Stored in `semantic` collection
- Updated asynchronously, never blocks a conversation turn

### Context Assembly (per LLM call)
```
System Prompt (persona layer — fixed, never from retrieval)
  + Semantic memory block (top-K relevant long-term facts)
  + Episodic memory block (top-K semantically similar past turns)
  + Recent turns window (last N raw turns, always included)
  + Current user input
```

The **persona layer is architecturally isolated** — it is never overwritten or diluted by retrieved memory. Memory is injected beneath it as context, not merged into it.

---

## Proactivity Mechanism

At the end of each LLM response, the model may optionally emit a structured deferred message tag:

```xml
<defer>
  {"message": "...", "after_minutes": 120, "trigger": "unresolved_topic"}
</defer>
```

The Rust orchestrator strips this tag before displaying the response. The scheduler (a persistent `tokio` task) stores deferred messages and fires them when their time condition is met. The frontend receives proactive messages through a Tauri event, not a user-initiated command.

---

## Audio Pipeline

```
[mic] → cpal (WASAPI/CoreAudio) → PCM buffer
     → whisper.cpp sidecar (STT)
     → transcript text → orchestrator

[orchestrator response text]
     → Kokoro TTS sidecar
     → PCM stream → cpal playback
```

- Use **cpal** for cross-platform audio I/O
- Stream Kokoro output as it generates — do not wait for full audio before playback
- Voice activity detection (VAD) must gate mic capture to avoid feeding silence to Whisper

---

## Debug & Monitoring UI

A dedicated debug panel is part of the MVP — not a future addition. With four sidecars, a vector DB, an audio pipeline, and a scheduler all running concurrently, you need system visibility from the first run. It lives as a togglable side panel or second tab in the React frontend, always available during development.

### What the Debug Panel Must Show

**Sidecar Health**
- Status indicator (green/yellow/red) per sidecar: llama.cpp, whisper.cpp, Kokoro, embedding instance
- Port, PID, uptime since last spawn
- Last HTTP response code and latency (ms) per sidecar — polled every 5s via a Tauri command

**Active Model**
- Currently loaded GGUF filename, parsed quantization type (Q4_K_M, Q5_K_S, etc.), parameter count from filename
- Context size configured vs. currently used (tokens)
- VRAM usage if queryable from llama.cpp `/health` or `/metrics` endpoint
- Tokens/sec from last inference (stored on `ModelResponse`)

**Memory Stats**
- Episodic collection: total entry count, last write timestamp
- Semantic collection: total entry count, last distillation run timestamp
- Embedding model: status, last embed latency (ms)
- Button: "Browse Memory" — opens `MemoryBrowser` panel showing recent episodic entries with their metadata and importance scores

**Audio Pipeline**
- Mic device name, sample rate, channel count
- VAD state: `silent` / `active` with real-time energy level bar
- Last STT result and its latency (ms)
- TTS: last synthesis latency, playback buffer fill %

**Scheduler**
- List of pending deferred messages: trigger text, scheduled fire time, remaining time
- Last fired message and when
- Button: "Fire Now" on any pending entry (dev shortcut to test proactivity without waiting)

**Context Inspector**
- Shows the fully assembled context sent on the last LLM call, broken into labeled sections: persona block, semantic block, episodic block, recent window, user input
- Token counts per section
- This is the single most useful debug tool — it makes context assembly bugs immediately visible

**Event Log**
- Streaming log of internal system events, newest at top
- Each entry: timestamp, component tag (`[MEMORY]`, `[AUDIO]`, `[SCHEDULER]`, `[ADAPTER]`, `[ORCHESTRATOR]`), message
- Filter bar by component tag
- Max 500 entries in memory, no persistence — this is a live debug view, not a log file
- Events emitted from Rust via `app_handle.emit("debug_event", payload)` — the frontend subscribes with `listen()`

### Model Management Panel

A dedicated tab (not buried in settings) for managing available models:

- Scans a configurable `models/` directory for `*.gguf` files
- Displays each model: filename, parsed quant type, file size, last modified
- "Load" button triggers `swap_model` Tauri command — shows loading spinner, updates active model display when confirmed
- Currently active model highlighted
- `// EXTEND:` placeholder row for future fine-tune management (disabled, labeled "Fine-tuning — future")

### New Tauri Commands Required

Add these to `commands.rs`:

- `get_system_status` → returns `SystemStatus` struct: sidecar health map, memory stats, scheduler queue, audio state
- `get_last_context` → returns the last `AssembledContext` as serialized JSON for the context inspector
- `fire_deferred_now(id: String)` → manually trigger a scheduled message by ID
- `list_models` → scans models directory, returns `Vec<ModelInfo>` with parsed metadata
- `get_debug_events` → returns last N events (frontend also receives them live via Tauri event channel)

### Rust-Side: `SystemStatus` Struct

```rust
#[derive(Serialize)]
pub struct SystemStatus {
    pub sidecars: HashMap<String, SidecarHealth>,
    pub active_model: ModelInfo,
    pub memory: MemoryStats,
    pub audio: AudioState,
    pub scheduler: SchedulerState,
}

#[derive(Serialize)]
pub struct SidecarHealth {
    pub alive: bool,
    pub port: u16,
    pub pid: Option<u32>,
    pub last_latency_ms: u64,
    pub last_status_code: u16,
}

#[derive(Serialize)]
pub struct MemoryStats {
    pub episodic_count: u64,
    pub semantic_count: u64,
    pub last_write: Option<DateTime<Utc>>,
    pub last_embed_latency_ms: u64,
}
```

---

## Project Structure to Generate

```
proactive-agent/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json            # sidecar entries for llama.cpp, whisper.cpp, kokoro
│   └── src/
│       ├── main.rs                # Tauri builder, setup, sidecar spawn
│       ├── commands.rs            # Tauri commands: chat, model_swap, memory_query,
│       │                          #   get_system_status, get_last_context,
│       │                          #   fire_deferred_now, list_models, get_debug_events
│       ├── orchestrator/
│       │   ├── mod.rs             # conversation loop, context assembly
│       │   ├── adapter.rs         # ModelAdapter trait + LlamaCppAdapter impl
│       │   ├── context.rs         # AssembledContext, context assembly logic
│       │   └── scheduler.rs      # proactivity scheduler (tokio task)
│       ├── memory/
│       │   ├── mod.rs
│       │   ├── episodic.rs        # turn processing, storage, retrieval
│       │   ├── semantic.rs        # long-term fact extraction, background distillation
│       │   └── embedding.rs      # EmbeddingService (locked to nomic-embed-text)
│       ├── audio/
│       │   ├── mod.rs
│       │   ├── capture.rs         # cpal mic capture + VAD
│       │   ├── stt.rs             # whisper.cpp sidecar client
│       │   └── tts.rs             # Kokoro sidecar client + streaming playback
│       ├── monitor.rs             # SystemStatus polling, sidecar health checks, event emitter
│       └── config.rs              # model paths, DB path, audio device selection
├── src/
│   ├── App.tsx                    # tab layout: Chat | Debug | Models
│   ├── components/
│   │   ├── chat/
│   │   │   ├── ChatWindow.tsx
│   │   │   └── WaveformVisualizer.tsx
│   │   ├── debug/
│   │   │   ├── DebugPanel.tsx     # root debug tab, assembles sub-panels
│   │   │   ├── SidecarHealth.tsx  # per-sidecar status indicators
│   │   │   ├── MemoryStats.tsx    # counts, timestamps, Browse button
│   │   │   ├── MemoryBrowser.tsx  # episodic entry list with metadata
│   │   │   ├── AudioState.tsx     # VAD bar, STT/TTS latencies
│   │   │   ├── SchedulerPanel.tsx # deferred message queue, Fire Now buttons
│   │   │   ├── ContextInspector.tsx # last assembled context, per-section token counts
│   │   │   └── EventLog.tsx       # live streaming event log with tag filter
│   │   └── models/
│   │       ├── ModelPanel.tsx     # model management tab root
│   │       ├── ModelList.tsx      # GGUF scanner, load button, active highlight
│   │       └── ModelSelector.tsx  # compact hot-swap widget reused in chat header
│   └── hooks/
│       ├── useChat.ts
│       ├── useProactiveEvents.ts  # listen for Tauri proactive message events
│       ├── useSystemStatus.ts     # polls get_system_status every 5s
│       └── useDebugEvents.ts      # subscribes to debug_event Tauri channel
└── binaries/                      # platform sidecar binaries (gitignored, populated by build script)
    ├── llama-server-x86_64-pc-windows-msvc.exe
    ├── llama-server-aarch64-apple-darwin
    ├── whisper-server-*
    └── kokoro-server-*
```

---

## Implementation Phases

This is the architecture document. Each phase is a discrete git commit. Implement one phase at a time — do not start the next phase until the current one is confirmed working.

All code must be real types, real imports, real structure — not pseudocode. Stubs are acceptable where logic is non-trivial, but types and signatures must be correct and `cargo check` must pass at the end of every Rust phase.

### Phase 1 — Project scaffold + Rust core
*Commit: `feat: phase 1 — project scaffold and Rust core`*

- `src-tauri/Cargo.toml` — dependencies: `tauri`, `lancedb`, `async-trait`, `cpal`, `tokio`, `serde`, `reqwest`, `anyhow`, `chrono`
- `src-tauri/tauri.conf.json` — sidecar entries for llama.cpp, whisper.cpp, kokoro
- `src-tauri/src/main.rs` — Tauri builder, sidecar spawn, tokio runtime setup
- `src-tauri/src/config.rs` — model paths, DB path, audio device selection, port config
- `src-tauri/src/orchestrator/adapter.rs` — `ModelAdapter` trait + `LlamaCppAdapter` against OpenAI-compatible REST; `ModelResponse` carries `tokens_per_sec`
- `src-tauri/src/orchestrator/context.rs` — `AssembledContext` struct and `assemble()` signature

Gate: `cargo check` passes. **Do not proceed to Phase 2 until confirmed.**

### Phase 2 — Memory layer
*Commit: `feat: phase 2 — memory layer (LanceDB, embeddings, episodic store)`*

- `src-tauri/src/memory/embedding.rs` — `EmbeddingService` locked to nomic-embed-text; tracks `last_embed_latency_ms`
- `src-tauri/src/memory/episodic.rs` — `EpisodicStore`: process turn, embed, store, retrieve top-K by similarity
- `src-tauri/src/memory/semantic.rs` — `SemanticStore`: background distillation stub, correct types and LanceDB schema

Gate: `cargo check` passes; store/retrieve round-trip compiles. **Do not proceed to Phase 3 until confirmed.**

### Phase 3 — Orchestration + Scheduler
*Commit: `feat: phase 3 — orchestrator, scheduler, and Tauri commands`*

- `src-tauri/src/orchestrator/mod.rs` — conversation loop: assemble context → call adapter → parse `<defer>` tags → store turn
- `src-tauri/src/orchestrator/scheduler.rs` — `ProactivityScheduler`: tokio interval loop, `Vec<DeferredMessage>` queue, fire via Tauri event
- `src-tauri/src/commands.rs` — all Tauri commands: `send_message`, `swap_model`, `get_memories`, `get_system_status`, `get_last_context`, `fire_deferred_now`, `list_models`, `get_debug_events`

Gate: `cargo check` passes; `send_message` wires end-to-end. **Do not proceed to Phase 4 until confirmed.**

### Phase 4 — Monitor + Audio
*Commit: `feat: phase 4 — system monitor and audio pipeline`*

- `src-tauri/src/monitor.rs` — `SystemStatus`, `SidecarHealth`, `MemoryStats`, `AudioState`, `SchedulerState`; health-check polling loop; `debug_event` emitter
- `src-tauri/src/audio/capture.rs` — cpal mic capture, energy-threshold VAD, exposes current energy level and VAD state
- `src-tauri/src/audio/stt.rs` — whisper.cpp HTTP client, returns transcript + latency
- `src-tauri/src/audio/tts.rs` — Kokoro chunked synthesis client + cpal streaming playback

Gate: `cargo check` passes. **Do not proceed to Phase 5 until confirmed.**

### Phase 5 — React core
*Commit: `feat: phase 5 — React shell, chat UI, model management`*

- `src/App.tsx` — three-tab shell: Chat | Debug | Models
- `src/hooks/useChat.ts`
- `src/hooks/useProactiveEvents.ts` — listens for Tauri proactive message events
- `src/components/chat/ChatWindow.tsx`
- `src/components/chat/WaveformVisualizer.tsx`
- `src/components/models/ModelPanel.tsx`, `ModelList.tsx`, `ModelSelector.tsx`

Gate: app renders, Chat tab functional against a running llama.cpp instance. **Do not proceed to Phase 6 until confirmed.**

### Phase 6 — Debug panel
*Commit: `feat: phase 6 — debug panel and observability UI`*

- `src/hooks/useSystemStatus.ts` — polls `get_system_status` every 5s
- `src/hooks/useDebugEvents.ts` — subscribes to `debug_event` Tauri channel
- `src/components/debug/DebugPanel.tsx` — root debug tab
- `src/components/debug/SidecarHealth.tsx`
- `src/components/debug/MemoryStats.tsx`, `MemoryBrowser.tsx`
- `src/components/debug/AudioState.tsx`
- `src/components/debug/SchedulerPanel.tsx` — deferred queue with "Fire Now" buttons
- `src/components/debug/ContextInspector.tsx` — last assembled context with per-section token counts
- `src/components/debug/EventLog.tsx` — live log with component tag filter

Gate: Debug tab renders all panels with live data from the Rust backend.

---

## Explicit Decisions Already Made — Do Not Revisit

- Vector DB: LanceDB (Rust crate, embedded, no server)
- Embedder: nomic-embed-text, fixed, never swapped
- Inference: llama.cpp REST (Vulkan/Windows, Metal/macOS)
- TTS: Kokoro
- STT: whisper.cpp
- Audio I/O: cpal
- Proactivity: LLM-emitted `<defer>` tags parsed by Rust scheduler
- Persona: fixed system prompt layer, isolated from memory retrieval
- Debug UI: part of MVP, three-tab layout (Chat / Debug / Models), not a future addition
- System health: polled every 5s via `get_system_status` + live events via `debug_event` channel
- Context inspector: always shows last assembled context with per-section token counts

---

## Style Directives

- Rust: idiomatic, use `anyhow::Result` everywhere, no `unwrap()` in non-test code
- Async: `tokio` throughout, `async_trait` for trait objects
- React: functional components, hooks only, no class components
- No over-engineering: MVP skeleton only, extension points clearly marked with `// EXTEND:` comments
- Be terse in comments — explain *why*, not *what*

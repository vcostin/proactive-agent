# Proactive Agent — Work Log

---

## What was built

### Phase 1 — Project scaffold + Rust core
- Tauri 2 project structure (Cargo.toml, tauri.conf.json, capabilities)
- `AppConfig` with all runtime parameters, persisted to `config.json` on disk
- `LlamaCppAdapter`: `ModelAdapter` trait + implementation against llama.cpp REST API.
  Includes auto-discovery of model ID via `GET /v1/models` with fallback alias
- `AssembledContext`: context assembly with persona/semantic/episodic/recent/input blocks,
  token counting, and `to_messages()` conversion to OpenAI format
- `monitor.rs`: `SystemStatus`, `SidecarHealth`, `MemoryStats`, `AudioState`, `SchedulerState`,
  `DeferredMessage`, `ModelInfo` structs
- `commands.rs`: all 8 Tauri commands stubbed with correct signatures

### Phase 2 — Memory layer
- `EmbeddingService`: HTTP client to llama.cpp `/v1/embeddings`, tracks latency
- `EpisodicStore`: LanceDB table, store/retrieve turns by vector similarity
- `SemanticStore`: LanceDB table for distilled long-term facts
- `MemoryStore`: shared connection, exposes count methods
- Arrow schema for both collections using `arrow-array 53` (matched to lancedb 0.14)
- LanceDB `count_rows()` wired into `get_system_status`

### Phase 3 — Orchestration + Scheduler
- Full conversation loop: embed → retrieve episodic+semantic → assemble context →
  call LLM → parse `<defer>` tags → store both turns → return response
- `ProactivityScheduler`: `Vec<DeferredMessage>` queue, `drain_due()` polled every 30s,
  fires via Tauri `emit("proactive_message")`
- `lib.rs`: `SharedOrchestrator` + `SharedScheduler` managed state, async init with
  `orchestrator_ready` / `init_error` Tauri events
- All commands wired: `send_message`, `swap_model`, `get_memories`, `get_system_status`,
  `get_last_context`, `fire_deferred_now`, `list_models`, `get_debug_events`

### Phase 4 — Monitor + Audio
- `monitor.rs`: `DebugEvent`, `SharedEventLog` ring buffer (500 entries),
  `emit_debug_event()` for live streaming to frontend, `run_monitor_loop()` health polling
- `AudioCapture`: cpal mic capture, energy-threshold VAD, non-blocking `try_send`
- `SttClient`: multipart WAV upload to whisper.cpp, `pcm_to_wav` encoder
- `TtsClient`: Kokoro sentence-chunked synthesis, cpal playback via `spawn_blocking`
- `run_stt_loop`: accumulates VAD frames, sends batch to Whisper on silence gap

### Phase 5 — React core
- `ChatWindow`: message bubbles, inline code/codeblock renderer, WaveformVisualizer,
  voice toggle, loading dots, proactive message display, auto-scroll
- `WaveformVisualizer`: canvas 24-bar energy display with decay animation
- `ModelPanel` + `ModelList`: native file dialog (`.gguf` from anywhere on disk),
  active model highlight, hot-swap, fine-tune placeholder row
- `ModelSelector`: compact hot-swap widget
- `useChat`: invoke `send_message`, optimistic user message, assistant response
- `useProactiveEvents`: listens for `proactive_message` Tauri events
- Dark monospace theme with CSS variables

### Phase 6 — Debug panel
- `SidecarHealth`: live `sidecar_health` events, green/red dots, latency display
- `MemoryStats`: row counts, timestamps, embed latency, Browse Memory button
- `MemoryBrowser`: search `get_memories`, shows episodic + semantic results
- `AudioState`: VAD energy bar with live colour, STT/TTS latency
- `SchedulerPanel`: pending deferred messages, remaining time, "Fire Now" button
- `ContextInspector`: last assembled context, per-section token counts + bars,
  collapsible content, total token bar vs context window
- `EventLog`: live `debug_event` channel + ring buffer seed, component tag filter
- `useSystemStatus`: polls every 5s, `useDebugEvents`: subscribes to live events

---

## Sidecar setup and deployment work

### Binary isolation
- Each sidecar runs in its own subdirectory (`binaries/llama/`, `binaries/whisper/`)
  to prevent DLL conflicts (llama.cpp and whisper.cpp both ship `ggml.dll`)
- `find_sidecar()` checks subdirectory naming conventions including short names
  (`llama-server` → `llama/`)

### DLL and binary debugging
- Diagnosed `STATUS_DLL_NOT_FOUND` (0xC0000135): recursive DLL copy from zip fixed
- Diagnosed `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139): resolved by isolating
  sidecar binaries into separate directories — was DLL version conflict between
  llama.cpp Vulkan build and other system DLLs
- Confirmed VCRedist 2022 was installed (green check), not the cause
- Root discovery: Vulkan-only `llama-server.exe` is a stripped build with only
  `/health` route. CPU build has full HTTP API. Fixed by using CPU binary + Vulkan DLLs

### Port collision (the big one)
- Port 8080 was occupied by LM Studio's background service the entire time
- Our llama-server silently failed to bind; the foreign process answered `/health`
  with 200 but had no API routes — all requests hit LM Studio, not our server
- Diagnostic: `chat child PID ≠ port 8080 owner PID`
- Fix: changed all ports to 18080-18083

### Self-contained setup
- `SetupWizard`: two-step first-run UI (required models download + chat model picker)
- `SystemRequirements`: auto-detects VCRedist, Vulkan, tests llama-server binary
- `install_vcredist`: downloads `vc_redist.x64.exe`, installs silently, copies fresh
  DLLs into `binaries/llama/`
- `RequirementsBanner`: startup check emits `system_deps_checked` event, shows red
  banner with Fix button when llama-server can't start
- Native file picker via `tauri-plugin-dialog` for chat model selection
- `download_required_models`: streams whisper + nomic-embed-text with progress bars
- Config persisted to `AppData/com.proactive.agent/config.json`

---

## Key bugs fixed (in order of frustration)

| Bug | Cause | Fix |
|-----|-------|-----|
| DLL crashes (STATUS_DLL_NOT_FOUND) | Flat binary layout mixed llama/whisper DLLs | Separate subdirectories |
| VCRedist mismatch (STATUS_ENTRYPOINT_NOT_FOUND) | Different DLL versions in `binaries/` root | DLL isolation + copy to subdirectory |
| Dialog permission denied | Missing `dialog:allow-open` capability | Added to capabilities/default.json |
| Dev model path `src-tauri/` instead of project root | `current_dir()` in dev returns `src-tauri/` | Detect and step up one level |
| Chat history lost on tab switch | Component unmount destroyed React state | CSS visibility — keep all tabs mounted |
| All chat requests → 404 | Port 8080 occupied by LM Studio; wrong process answering | Changed ports to 18080-18083 |
| Vulkan llama-server: only `/health` works | Vulkan-only build is stripped (HTTP API not compiled) | Use CPU build binary + Vulkan DLLs |
| Model ID 404 after port fix | `/v1/models` returns `{"models":[]}` not `{"data":[]}` | Updated parser to handle both formats |

---

## Current state (end of session)

- ✅ Chat works (Meta-Llama-3.1-8B-Instruct-Q4_K_S on AMD RX 7800 XT via Vulkan)
- ✅ Vector memory (LanceDB + nomic-embed-text, 78ms embed latency)
- ✅ Context inspector showing assembled context per call
- ✅ Debug panel with live event log, sidecar health, scheduler, memory browser
- ✅ Model hot-swap without restart
- ✅ System requirements check + auto-install
- ✅ Self-contained first-run wizard
- ⬜ Voice input not wired (audio pipeline built, not connected)
- ⬜ Kokoro TTS placeholder
- ⬜ Whisper exits after load (port binding issue)
- ⬜ Streaming responses not implemented
- ⬜ Chat history not persisted to disk

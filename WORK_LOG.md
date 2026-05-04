# Proactive Agent — Work Log

---

## What was built

### Phase 1 — Project scaffold + Rust core
- Tauri 2 project structure (Cargo.toml, tauri.conf.json, capabilities)
- `AppConfig` with all runtime parameters, persisted to `config.json`
- `LlamaCppAdapter`: `ModelAdapter` trait + auto-discovery of model ID via `GET /v1/models`
  (handles both llama.cpp `{"models":[]}` and OpenAI `{"data":[]}` formats)
- `AssembledContext`: persona/semantic/episodic/recent/input blocks with token counting
- Monitor structs: `SystemStatus`, `SidecarHealth`, `MemoryStats`, `AudioState` etc.
- All Tauri commands scaffolded

### Phase 2 — Memory layer
- `EmbeddingService`: HTTP client to `/v1/embeddings`, tracks latency
- `EpisodicStore`: LanceDB table, vector similarity search, row count, recent retrieval
- `SemanticStore`: LanceDB table for distilled facts, LLM-based distillation
- Arrow schema using `arrow-array 53` (matched to lancedb 0.14)

### Phase 3 — Orchestration + Scheduler
- Full conversation loop: embed → retrieve memories → trim to context window →
  assemble context → stream LLM → parse `<defer>` tags → store turns
- `ProactivityScheduler`: deferred message queue, 30s polling, `fire_now` for debug
- Semantic distillation background task (every 10 min)
- All Tauri commands wired

### Phase 4 — Monitor + Audio
- `monitor.rs`: `DebugEvent` ring buffer (500), `emit_debug_event`, health polling
- `AudioCapture`: cpal mic + energy VAD on dedicated thread (cpal::Stream is !Send)
- `SttClient`: multipart WAV upload to whisper.cpp
- `TtsClient`: Kokoro chunked synthesis + cpal playback
- `run_stt_loop`: accumulates VAD frames, transcribes on silence, emits `voice_transcript`

### Phase 5 — React core
- `ChatWindow`: streaming bubbles with blinking cursor, voice toggle, loading state,
  clear history button, proactive message display
- `WaveformVisualizer`: canvas 24-bar energy display
- `ModelPanel` + parameter controls: temperature/top-p/context sliders, hot-swap
- `useChat`: streaming via `chat_token` events, history in localStorage, voice routing
- `useLlamaReady`: watches `sidecar_health` events for port 18080
- Dark monospace theme

### Phase 6 — Debug panel
- `SidecarHealth`: live green/red dots from `sidecar_health` events
- `MemoryStats`, `MemoryBrowser`, `AudioState`, `SchedulerPanel`
- `ContextInspector`: last assembled context, per-section token counts, collapsible
- `EventLog`: live `debug_event` channel + ring buffer, component tag filter
- `DiagnosticButton`: port 8080/18080 probe with process name lookup

---

## Sidecar + infrastructure work

### Binary isolation
- Each sidecar in its own subdirectory (`binaries/llama/`, `binaries/whisper/`)
  to prevent DLL conflicts (llama.cpp and whisper.cpp both ship `ggml.dll`)
- `find_sidecar()` checks subdirectory naming including short names

### CPU binary + Vulkan DLLs
- Vulkan-only `llama-server.exe` has a stripped HTTP server (only `/health`)
- CPU build has full API. CPU binary + Vulkan DLLs in same dir = full API + GPU inference
  (llama.cpp loads backends dynamically via DLL names from the exe's directory)
- `fetch-sidecars-windows.ps1` downloads CPU binary + Vulkan DLLs separately

### Port collision (root cause of weeks of debugging)
- Port 8080 was occupied by LM Studio's background service the entire time
- Our llama-server silently failed to bind; the foreign process answered `/health` 200
  but had no API routes — all requests were hitting LM Studio, not our server
- Diagnostic: `chat child PID ≠ port 8080 owner PID`
- Fix: changed all ports to 18080-18083

### Self-contained setup
- `SetupWizard` with system requirements check (VCRedist auto-install, Vulkan detection)
- `RequirementsBanner` on every launch when llama-server test fails
- In-app download of whisper + nomic-embed-text models with progress bars
- Native file picker for chat model selection
- Config persisted to OS app data dir

---

## Key bugs fixed (chronological)

| Bug | Root cause | Fix |
|-----|-----------|-----|
| DLL crashes (STATUS_DLL_NOT_FOUND) | Flat binary layout mixed llama/whisper DLLs | Separate subdirectories |
| VCRedist mismatch (ENTRYPOINT_NOT_FOUND) | DLL version conflict between builds | DLL isolation per sidecar |
| Dialog permission denied | Missing `dialog:allow-open` capability | Added to capabilities |
| Dev model path wrong | `current_dir()` returns `src-tauri/` in dev mode | Detect and step up one level |
| All API routes 404 (weeks of debugging) | Port 8080 taken by LM Studio; wrong PID | Changed ports to 18080-18083 |
| Vulkan llama-server: only /health works | Vulkan build is stripped (HTTP API not compiled) | Use CPU build binary + Vulkan DLLs |
| Model ID 404 after port fix | `/v1/models` returns `{"models":[]}` not `{"data":[]}` | Updated parser for both formats |
| Chat history lost on tab switch | Component unmount destroyed React state | CSS visibility — keep all tabs mounted |
| Chat token streaming freeze | `block_in_place` + `block_on` deadlock in async adapter | Replaced with clean async SSE parsing |
| cpal::Stream !Send in managed state | WASAPI Stream uses COM pointers | Dedicated `std::thread` for audio capture |

---

## Current state

### ✅ Working
- Chat inference (Meta-Llama-3.1-8B via Vulkan, port 18080)
- Streaming responses with live token display
- Vector memory (LanceDB + nomic-embed-text 78ms embed latency)
- Semantic memory distillation (background, every 10 min)
- Context window overflow protection
- Debug panel (event log, context inspector, sidecar health, scheduler)
- Model hot-swap without restart
- System requirements check + VCRedist auto-install
- First-run setup wizard with model download
- Chat history persistence (localStorage)
- Loading state spinner while model warms up
- Voice input wiring (mic → AudioCapture → STT → chat)
- Model parameters: temperature, top-p, context window (UI + persisted)

### ⬜ Outstanding
- Whisper exits after model load (port binding investigation ongoing)
- Kokoro TTS (placeholder binary — best path: sherpa-onnx pre-built)
- `<defer>` proactivity needs model output verification
- Production build + real app icon
- macOS testing
- `.gitattributes` for CRLF warnings

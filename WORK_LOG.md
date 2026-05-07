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
- `SttClient`: HTTP POST to Parakeet TDT server `/v1/audio/transcriptions`
- `TtsClient`: Piper TTS subprocess (text→stdin, WAV→file, cpal playback)
- `run_stt_loop`: accumulates VAD frames, transcribes on silence, emits `voice_transcript`

### Phase 5 — React core
- `ChatWindow`: streaming bubbles with blinking cursor, voice toggle, loading state,
  reset memory button (requires typing RESET), proactive message display
- `WaveformVisualizer`: canvas animation, reacts to live mic energy (polling)
- `ModelPanel` + parameter controls: temperature/top-p/context sliders, hot-swap
- `useChat`: streaming via `chat_token` events, history in localStorage, voice routing,
  ttsEnabledRef (fixes closure capture bug)
- `useLlamaReady`: watches `sidecar_health` events for port 18080
- `useAudioEnergy`: polls `get_audio_energy` every 50ms while listening

### Phase 6 — Debug panel
- `SidecarHealth`: live green/red dots from `sidecar_health` events
- `MemoryStats`, `MemoryBrowser`, `AudioState`, `SchedulerPanel`
- `ContextInspector`: last assembled context, per-section token counts, auto-refreshes
- `EventLog`: live `debug_event` channel + ring buffer, component tag filter
- Dev tools section (collapsed): Port Diagnostic + TTS Test button

---

## Sidecar architecture

### Binary layout
```
binaries/
  llama/          ← CPU build exe + Vulkan DLLs (GPU inference via dynamic loading)
  parakeet/       ← parakeet-server-x86_64-pc-windows-msvc.exe (Python frozen binary)
  piper/          ← piper.exe + onnxruntime.dll + espeak-ng-data/ + piper_phonemize.dll
  kokoro/         ← 21MB binary (sherpa-onnx, currently unused — TTS moved to piper subprocess)
  whisper/        ← whisper-server (retired, replaced by Parakeet)
```

### Port assignments
| Service | Port | Notes |
|---------|------|-------|
| llama (chat) | 18080 | CPU binary, Vulkan GPU via DLLs, --alias llama-chat |
| embed | 18081 | --embedding flag, --alias nomic-embed-text |
| parakeet STT | 5092 | Hardcoded in parakeet's app.py, can't change via CLI |
| kokoro (unused) | 18083 | Port reserved but not used |

### Key discovery: why Vulkan build doesn't work as HTTP server
The Vulkan-only `llama-server.exe` has a stripped HTTP server — only `/health` route.
The CPU build has the full API. Using CPU binary + Vulkan DLLs gives both:
full HTTP API AND GPU inference (llama.cpp loads backends dynamically from DLLs).

### Key discovery: port 8080 collision
LM Studio background service occupies port 8080. Our servers were silently failing
to bind. Changed all ports to 18080-18083 range.

### Key discovery: parakeet port
parakeet-server port is hardcoded at 5092 in app.py line 4. Cannot be changed
via CLI. Config `stt_port = 5092` must match.

---

## Audio pipeline

### STT (speech-to-text)
```
Mic (48kHz stereo) → cpal capture → energy VAD (threshold 0.005)
→ accumulate frames → 800ms silence gap
→ downmix stereo→mono → amplify (normalize to 0.7 peak)
→ multipart WAV POST to parakeet:5092/v1/audio/transcriptions
→ voice_transcript Tauri event → sendMessage()
```
- Parakeet TDT 0.6B (ONNX, Python frozen binary) — multilingual, auto-detected
- Model downloads from HF on first run (server self-downloads)
- Clean_transcript() filters hallucinations: [BLANK_AUDIO], (Music), etc.

### TTS (text-to-speech)
```
assistant response → clean markdown → Piper subprocess (stdin→WAV file)
→ parse WAV header (22050Hz, 1ch) → resample to device rate (linear interp)
→ upmix mono→stereo → cpal playback
```
- Piper binary: `binaries/piper/piper-x86_64-pc-windows-msvc.exe`
- Voice model: `models/tts/en_US-lessac-medium.onnx` + `.onnx.json`
- 🔊/🔇 toggle in chat header, off by default
- Known issue: TTS speed was 2x due to sample rate (22050→44100/48000) and
  mono→stereo mismatch. Both fixed by resampling and channel duplication.

---

## Key bugs fixed (comprehensive)

| Bug | Root cause | Fix |
|-----|-----------|-----|
| DLL crashes STATUS_DLL_NOT_FOUND | Flat binary layout mixed llama/whisper DLLs | Separate subdirectories |
| STATUS_ENTRYPOINT_NOT_FOUND | DLL version conflict + VCRedist outdated | DLL isolation + VCRedist install |
| Dialog permission denied | Missing `dialog:allow-open` capability | Added to capabilities |
| Dev model path wrong | `current_dir()` returns `src-tauri/` | Detect and step up one level |
| All API routes 404 (weeks of debugging) | Port 8080 taken by LM Studio | Changed to 18080-18083 |
| Vulkan llama-server: only /health | Vulkan build is stripped HTTP | Use CPU binary + Vulkan DLLs |
| Model ID 404 | `/v1/models` returns `{"models":[]}` not `{"data":[]}` | Updated parser both formats |
| Chat history lost on tab switch | Component unmount destroyed React state | CSS visibility keep all tabs mounted |
| TTS not firing (silent) | `ttsEnabled` stale in useCallback closure | Use `ttsEnabledRef` updated via useEffect |
| TTS logs invisible | `push_event` only writes ring buffer, no Tauri event | Use `emit_debug_event` everywhere |
| TTS 2x speed | 22050Hz WAV played through 44100/48000Hz device | Resample WAV to device rate |
| TTS still fast | Mono WAV fed into stereo cpal buffer | Upmix mono→stereo (duplicate L/R) |
| Parakeet browser tab | `webbrowser.open_new_tab` in app.py startup | Patched + rebuilt binary |
| cpal::Stream !Send | WASAPI Stream can't cross threads | Dedicated `std::thread` for audio capture |
| Graceful shutdown (DLLs locked) | Sidecars kept running after app close | `kill_all_sidecars()` on `RunEvent::Exit` |

---

## Current state

### ✅ Working
- Chat inference (Meta-Llama-3.1-8B via Vulkan, port 18080)
- Streaming responses with live token display + blinking cursor
- Vector memory (LanceDB + nomic-embed-text, ~78ms embed latency)
- Semantic memory distillation (background, every 10 min)
- Context window overflow protection (trims oldest episodic first)
- Debug panel (event log, context inspector, sidecar health, scheduler)
- Model hot-swap without restart
- System requirements check + VCRedist auto-install
- First-run setup wizard with model download + progress
- Chat history persistence (localStorage)
- Loading state (status dot, green=ready)
- Voice input (mic → Parakeet STT → chat)
- TTS voice output (🔊 toggle → Piper → cpal)
- Memory reset (types RESET to confirm, wipes LanceDB + recent turns)
- Graceful shutdown (sidecars killed, DLLs released)
- Model parameters (temperature/top-p/context sliders)

### ⬜ Outstanding
- TTS speed not yet verified at correct rate (latest fix just committed)
- Production build test (`npm run tauri build`)
- Real app icon (currently blue P on dark background from app-icon.svg)
- macOS support (fetch script exists, untested)
- STT accuracy (Parakeet 0.6B struggles with non-native accents — model limitation)
</content>
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

## Episodic memory role-blurring bug (documented retrospectively)

**The bug:** The LLM started confusing its own words with the user's words mid-conversation.
It began responding as if it were the user, losing track of which conversation turns
belonged to which participant. The user described it as "switching roles."

**Root cause:** User turns and assistant turns were being stored in episodic memory without
a role label, then retrieved as an undifferentiated block and injected into context.
The LLM received a wall of past text with no signal about who said what — so it guessed,
and guessed wrong.

**User diagnosis (from live conversation):**
> "It might be also the vector database problem, so you don't know what's yours and what's mine."

This was correct. The model responded:
> "It's almost as if our mental models are getting tangled together, making it difficult
> to distinguish between what's mine and what's yours."

**Fix:** Every episodic entry now carries a `role` field (`user` | `assistant`).
Retrieval returns role-labeled turns. Context assembly injects them with explicit
role prefixes so the LLM always knows whose words are whose.

**Why this matters for future changes:** Any modification to `memory/episodic.rs`
that removes, ignores, or flattens the role field will reintroduce this bug.
The SUPERVISOR.md hard-stop list encodes this invariant explicitly.

---

## Wizard-owned binary downloads — architecture decision

### The problem that triggered it

Running `npm run tauri dev` with an empty `binaries/` folder failed at **compile time**
with `resource path does not exist`. The Tauri build script validates all `externalBin`
and `resources` entries before Cargo even starts compiling. This made it impossible to
launch the app to reach the wizard that would have downloaded the missing binaries —
a classic chicken-and-egg problem.

The first attempt was stub files: create 0-byte placeholders to satisfy the build check,
then let the wizard replace them with real binaries. This was rejected as the wrong
approach — stubs are a symptom fix that papers over a structural problem.

### The real problem

The root cause was that `llama-server` and `piper` were listed in `externalBin` and
`resources` in `tauri.conf.json`. This told Tauri to:
1. Validate they exist at build time
2. Bundle them into the installer

Both of those are wrong if the wizard is responsible for downloading them. Having the
installer bundle them AND having the wizard download them is doing the same job twice,
and creates the compile-time dependency that broke the dev workflow.

### The fix

**`llama-server` and `piper` removed from `externalBin` and `resources`.**
Only `parakeet-server` remains in `externalBin` — it is the one binary that has no
public download URL and must be manually provided and bundled.

Consequences:
- No compile-time check for llama or piper — the build succeeds with an empty `binaries/`
- The installer no longer bundles them — installer is smaller
- The wizard is the single source of truth for getting them onto the machine

### binaries_dir() in release — AppData not exe directory

Removing them from `externalBin` means the installer no longer places them alongside
the exe. The wizard needs somewhere to download them. The exe directory
(`C:\Program Files\proactive-agent\`) requires admin rights to write — not acceptable.

**Release `binaries_dir()` now returns AppData:**
- Windows: `%APPDATA%\com.proactive.agent\binaries\`
- macOS: `~/Library/Application Support/com.proactive.agent/binaries/`
- Linux: `~/.local/share/com.proactive.agent/binaries/`

This is user-writable, matches the path Tauri already uses for `config.json`, and
survives app updates without needing admin rights.

### find_sidecar() — checks both AppData and exe directory

Parakeet is still bundled via `externalBin` and placed next to the exe by the installer.
`find_sidecar()` in release now checks both:
1. AppData binaries dir — wizard-downloaded llama and piper
2. Exe directory — installer-bundled parakeet

Dev mode is unchanged: `npm run setup` downloads to `project/binaries/` and
`find_sidecar()` looks there.

### Dev workflow

```
npm run setup        # downloads llama + piper + models to project/binaries/
npm run tauri dev    # works even with empty binaries/ — no compile-time check
```

### Production workflow (end user)

```
Install .msi / setup.exe      # installs app + parakeet only
Launch proactive-agent        # wizard appears
Step 1: Downloads llama-server + piper to AppData (no admin rights)
Step 2: Downloads nomic-embed-text + Parakeet ONNX models
Step 3: User picks a .gguf chat model
```

---

## Component history — why Whisper and Kokoro were replaced

The original architecture used **whisper.cpp** (STT) and **Kokoro TTS** (sherpa-onnx).
Both were replaced in an earlier session that predates this work log.

**What is documented:**
- The flat `binaries/` layout that co-located whisper and llama DLLs caused
  `STATUS_DLL_NOT_FOUND` and `STATUS_ENTRYPOINT_NOT_FOUND` crashes — DLL version
  conflicts between the two sidecars. Solved by isolating each into its own subdirectory.
- Kokoro was running as a long-running HTTP server (`kokoro-server`) but TTS was later
  moved to a per-request Piper subprocess, which removed the server entirely.
- The Kokoro sidecar health check was still pinging a server that no longer existed,
  producing a permanent red dot in the debug panel. Removed in cleanup.

**What is not documented:**
The specific failure mode of Kokoro TTS was not recorded.

**What is now known (added retrospectively):**
The primary reason for replacing whisper.cpp was **STT accuracy** — it could not
reliably transcribe the developer's voice. Recognition quality was poor enough to make
voice input unusable in practice ("a disaster"). No amount of tuning resolved it.

Parakeet TDT is a meaningful improvement in usability — transcription is far more
intuitive — but it is still not perfect, particularly on non-native accents (which is
a known model limitation of the 0.6B variant, not a configuration problem).

This context matters for future STT decisions: the bar is not "perfect accuracy" but
"accurate enough to be usable without frustration." Whisper failed that bar entirely.
Parakeet clears it. If Parakeet is ever reconsidered, the replacement must demonstrably
outperform it on the same voice, not just on benchmarks.

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
- TTS voice output (🔊 toggle → Piper → cpal) — speed fix confirmed in code (resample + stereo upmix)
- Memory reset (types RESET to confirm, wipes LanceDB + recent turns)
- Graceful shutdown (sidecars killed, DLLs released)
- Model parameters (temperature/top-p/context sliders)

### ⬜ Outstanding
- Production installer gaps: `espeak-ng-data/` (piper phoneme data), `ggml-cpu-*.dll` backends, `libomp140`
- macOS support (fetch script exists, untested)
- STT accuracy (Parakeet 0.6B struggles with non-native accents — model limitation)
- Frontend tests: Vitest + `@tauri-apps/api/mocks` scaffolding
</content>
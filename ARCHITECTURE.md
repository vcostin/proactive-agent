# Proactive Agent — Architecture

Current implementation as of July 2026. Describes what is actually built and running,
not aspirational design. For planned work see `ROADMAP.md`.

---

## Hardware boundary decision (locked)

```
GPU (VRAM)  ──►  LLM inference only
                 Reserved entirely for the chat model.
                 Larger future models need headroom — nothing else competes for VRAM.

CPU         ──►  Everything else:
                 STT  — Parakeet ONNX on CPU (today: HTTP sidecar; target: ort in-process)
                 TTS  — Piper subprocess
                 Embeddings — nomic-embed-text via llama-server CPU path
                 LanceDB — embedded, CPU/disk
```

This is a hard constraint. Do not route audio inference through the GPU even if it
would be faster — the LLM needs the budget headroom for 14B, 32B and beyond.

---

## System overview

```
┌─────────────────────────────────────────────────────────┐
│                     Tauri 2 shell                       │
│  ┌──────────────────────┐  ┌──────────────────────────┐ │
│  │   React frontend     │  │      Rust backend        │ │
│  │  (TypeScript + Vite) │  │      (tokio async)       │ │
│  └──────────┬───────────┘  └─────────────┬────────────┘ │
│             │    invoke / event           │              │
│             └─────────────────────────────┘             │
└────────────────────────────┬────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │ HTTP               │ HTTP (:5092)       │ subprocess
 ┌──────▼──────┐      ┌──────▼──────┐      ┌──────▼──────┐
 │ llama-server│      │  Parakeet   │      │    piper    │
 │  :18080     │      │  STT sidecar│      │  (TTS, CPU) │
 │  :18081     │      │  (CPU ONNX) │      │             │
 │ (GPU, VRAM) │      │  → ort soon │      │             │
 └──────┬──────┘      └─────────────┘      └─────────────┘
        │
 ┌──────▼──────┐
 │  LanceDB    │
 │  (embedded) │
 └─────────────┘
```

JS tooling: **Deno preferred** (`deno.json`); `package.json` / npm still supported.
---

## Component inventory

### 1. Inference — llama-server

| | |
|--|--|
| **Binary** | `llama-server-x86_64-pc-windows-msvc.exe` (CPU build) |
| **Source** | [ggerganov/llama.cpp](https://github.com/ggerganov/llama.cpp) |
| **GPU** | Vulkan backend loaded dynamically from `ggml-vulkan.dll` |
| **Ports** | 18080 (chat), 18081 (embeddings) |
| **API** | OpenAI-compatible REST (`/v1/chat/completions`, `/v1/embeddings`) |
| **Streaming** | SSE token stream — Rust reads chunks and emits `chat_token` Tauri events |

**Why two ports?** The embedding model (`nomic-embed-text`) must never be swapped,
even when the chat model changes. Running it on a separate port with `--alias nomic-embed-text`
enforces this — the embedding service is a fixed infrastructure component, not a chat model.

**GPU strategy:** The CPU build has a full HTTP server. The Vulkan-only build has only
`/health`. Using the CPU binary + Vulkan DLLs alongside it gives both: full API AND
GPU inference (llama.cpp loads backends dynamically via `LoadLibrary`).

**Alternatives if switching:**

| Alternative | Trade-off |
|-------------|-----------|
| **Ollama** | Easier model management, larger install, no fine-grained port control |
| **llama.cpp server (Metal, macOS)** | Same binary, different backend flag — already planned |
| **llama.cpp server (CUDA)** | Swap Vulkan DLLs for CUDA. Faster on NVIDIA with CUDA drivers |
| **LM Studio local server** | Good UX, closed source, port 1234 — conflicts with common defaults |
| **vllm / text-generation-inference** | Production-grade, requires Python + heavy GPU, overkill for desktop |
| **OpenAI API (cloud)** | Drop `LlamaCppAdapter`, implement `OpenAiAdapter` — the `ModelAdapter` trait supports this |

The `ModelAdapter` trait in `orchestrator/adapter.rs` is the abstraction boundary.
Switching inference backends requires only a new `impl ModelAdapter`, not touching the
orchestrator or frontend.

---

### 2. Speech-to-text — Parakeet TDT (HTTP sidecar today → ort next)

**Current:** HTTP sidecar on `:5092`
- **Windows:** `parakeet-server-….exe` (PyInstaller frozen Python), when present
- **Linux:** small shell launcher under `binaries/parakeet/` written by
  `scripts/run-parakeet-linux.sh` (`deno task setup` / `deno task parakeet:linux`).
  Real work runs in `.cache/parakeet-tdt/` (uv venv, CPU ONNX, `PARAKEET_USE_GPU=false`).
  The app auto-spawns the launcher on startup if `/healthz` is not already up.

**Target:** Parakeet ONNX loaded in-process via the `ort` Rust crate — no Python, no
subprocess, no port. See `STT_ORT_MIGRATION.md`.

| | Current | Target |
|--|---------|--------|
| **Runtime** | HTTP sidecar (frozen exe or managed launcher) | `ort` Rust crate in-process |
| **Model** | Parakeet TDT 0.6B ONNX | Same model, same file |
| **Port** | 5092 (hardcoded in upstream server) | None — direct function call |
| **Python** | Required for sidecar | Eliminated |
| **CPU/GPU** | CPU only | CPU only (deliberate — GPU reserved for LLM) |
| **Cross-platform** | Per-OS launcher/binary | Single ONNX file |

**Pipeline (current):**
`48kHz stereo → VAD → rubato 16kHz → WAV → HTTP POST → sidecar ONNX → text`

**Pipeline (target):**
`48kHz stereo → VAD → rubato 16kHz → log-mel spectrogram → ort tensor → Parakeet ONNX → CTC decode → text`

**What changes for ort:** Primarily `audio/stt.rs` — `SttClient` holds an `ort::Session`
instead of a `reqwest::Client`. Upstream (VAD, rubato, normalization) stays identical.
Linux launcher scripts and spawn path go away with the migration.

**Whisper is retired. Do not revisit it.** Lower accuracy on non-native accents confirmed.
See WORK_LOG § Episodic memory role-blurring bug for context.

---

### 3. Text-to-speech — Piper

| | |
|--|--|
| **Binary** | `piper-x86_64-pc-windows-msvc.exe` |
| **Source** | [rhasspy/piper v2023.11.14-2](https://github.com/rhasspy/piper/releases) |
| **Voice model** | `en_US-lessac-medium.onnx` — good quality, natural cadence |
| **Mode** | Subprocess per utterance (not a persistent server) |
| **Pipeline** | text → `stdin` → piper → WAV file → parse header → resample → stereo upmix → cpal |

**Audio path detail:**
1. Piper writes a WAV at 22050 Hz mono
2. `wav_to_f32()` reads sample rate and channels from the WAV header
3. `resample()` linear-interpolates to the device output rate (typically 44100 or 48000 Hz)
4. Upmix: each mono sample is duplicated to fill L and R channels
5. `cpal` plays the f32 buffer through the default output device

Previously two bugs here: the WAV was played at 2× speed because the sample rate was
not read from the header (assumed 44100), and mono samples were fed directly into a
stereo buffer. Both fixed — covered by 13 unit tests in `audio/tts.rs`.

**Required files alongside `piper.exe`:**
- `espeak-ng.dll` — phonemizer
- `piper_phonemize.dll` — phoneme → IPA conversion
- `onnxruntime.dll` + `onnxruntime_providers_shared.dll` — ONNX inference
- `espeak-ng-data/` — language phoneme dictionaries (required, ~10 MB)

**Alternatives if switching:**

| Alternative | Trade-off |
|-------------|-----------|
| **Different Piper voice** | Drop-in — change the `.onnx` model file and `.json` config. 900+ voices at [rhasspy/piper-voices](https://github.com/rhasspy/piper-voices) |
| **Piper newer release** | Update the download URL in `fetch-sidecars-windows.ps1`. Retest WAV format compatibility |
| **Kokoro TTS** | Was the original TTS plan. Sherpa-onnx based. Retired — see WORK_LOG. Could be revisited; quality is competitive |
| **Coqui TTS / XTTS** | Much higher quality and voice cloning. Python-based, heavy (~1 GB), requires GPU for real-time |
| **Edge-TTS (Microsoft)** | Extremely natural voices, free, but requires internet (cloud call) |
| **System TTS (SAPI / AVSpeech)** | Zero dependencies, OS-native. Robotic quality on Windows |
| **Chatterbox / OuteTTS** | Newer open-source options, ONNX-compatible — untested |

---

### 4. Vector memory — LanceDB

| | |
|--|--|
| **Crate** | `lancedb 0.14` (pinned — API has had breaking changes) |
| **Storage** | `%APPDATA%\com.proactive.agent\memory\` (release) or `data/memory/` (dev) |
| **Tables** | `episodic` (conversation turns), `semantic` (distilled long-term facts) |
| **Embedder** | `nomic-embed-text-v1.5.Q8_0.gguf` via llama-server :18081 |
| **Schema** | Arrow arrays: `id` (String), `text` (String), `embedding` (FixedSizeList<f32, 768>), `timestamp` (Utf8) |

**Why two tables?** Episodic memory is high-frequency and noisy — every turn goes in.
Semantic memory is low-frequency and curated — a background task distills episodic
entries into durable facts every 10 minutes. Retrieval blends both: top-K semantic
for stable context, top-K episodic for recent relevance.

**Embedding model is locked.** `nomic-embed-text` produces 768-dimensional vectors.
If you change the embedder, all existing vectors become incompatible. The schema encodes
the dimensionality — a model swap requires a full memory migration or reset.

**Alternatives if switching:**

| Alternative | Trade-off |
|-------------|-----------|
| **SQLite + sqlite-vss** | Much simpler dependency, approximate vector search via FAISS extension. No Arrow/DataFusion overhead |
| **Qdrant (embedded)** | Better vector search features, more complex build |
| **Milvus Lite** | Production-grade, but heavy |
| **In-memory (HashMap)** | No persistence. Acceptable for a session-only prototype |
| **Different embedding model** | Must be 768-dim to keep the schema, OR migrate all stored vectors. `all-minilm-l6-v2` (384-dim) is smaller but requires schema change |

---

### 5. Audio I/O — cpal

| | |
|--|--|
| **Crate** | `cpal 0.15` |
| **Capture** | Default input device, 48000 Hz stereo (WASAPI on Windows) |
| **Playback** | Default output device, device-native rate (typically 44100 or 48000 Hz) |
| **Threading** | `cpal::Stream` is `!Send` — mic capture runs on a dedicated `std::thread`, not inside `tokio` |

**Alternatives if switching:**

| Alternative | Trade-off |
|-------------|-----------|
| **rodio** | Higher-level, built on cpal. Simpler playback API, less control over raw PCM |
| **portaudio** | Cross-platform C library via FFI. More mature but C dependency |
| **Web Audio API** | Frontend-only approach — capture in JS, send PCM to Rust via Tauri command. Avoids cpal entirely but adds latency |

---

### 6. Frontend — React + Vite + Tauri 2

| | |
|--|--|
| **Framework** | React 18, functional components + hooks only |
| **Build** | Vite 5, TypeScript strict |
| **IPC** | `@tauri-apps/api` `invoke` (commands) + `listen` (events) |
| **Styling** | Plain CSS variables — no CSS framework |
| **State** | React state + `useRef` for closure-capture-sensitive values (e.g. `ttsEnabledRef`) |

**No global state library.** The app is simple enough that prop drilling + local state
suffices. If the component tree grows, Zustand would be the first choice (tiny, no boilerplate).

**Key hook patterns:**

```
useChat          — streaming token assembly, history (localStorage), TTS routing
useLlamaReady    — watches sidecar_health events, gates UI until port 18080 is alive
useAudioEnergy   — polls get_audio_energy every 50ms for waveform visualizer
```

**Invoke/event boundary:** All Rust state lives behind `invoke()` calls or Tauri events.
The frontend has no direct access to the filesystem, processes, or LanceDB.
Consider adding `tauri-specta` when the command surface stabilises — it generates
TypeScript types from Rust signatures, making `tsc` enforce the IPC contract.

---

## Data flows

### Conversation turn

```
User types / speaks
  → [STT: mic → parakeet → text]      (if voice enabled)
  → sendMessage(text)                  invoke('send_message')
  → embed(text)                        POST :18081/v1/embeddings
  → retrieve top-K episodic + semantic  LanceDB similarity search
  → trim context to window             token counting + oldest-first eviction
  → assemble context                   persona + semantic + episodic + recent + input
  → POST :18080/v1/chat/completions    streaming SSE
  → parse <defer> tags                 strip from display, queue in scheduler
  → emit chat_token events             Tauri → React streaming display
  → store turn in episodic             embed + LanceDB insert
  → [TTS: text → piper → cpal]        (if voice enabled)
```

### Proactive message

```
Scheduler tick (every 30s)
  → check pending deferred messages
  → fire_at <= now?
  → emit proactive_message Tauri event
  → React renders as assistant bubble
  → full conversation turn triggered (same path as above, minus user input)
```

### Semantic distillation (background)

```
Every 10 minutes
  → fetch recent episodic entries not yet distilled
  → POST :18080/v1/chat/completions   "extract key facts from these turns"
  → parse response into fact list
  → upsert into semantic table
```

---

## Rust module map

```
src-tauri/src/
├── lib.rs              App setup, sidecar spawn, graceful shutdown, find_sidecar()
├── main.rs             Tauri builder entry point
├── config.rs           AppConfig struct — persisted to config.json via serde_json
├── commands.rs         All #[tauri::command] handlers
├── monitor.rs          Health polling loop, SharedEventLog, emit_debug_event()
├── orchestrator/
│   ├── mod.rs          run_conversation() — the main chat loop
│   ├── adapter.rs      ModelAdapter trait + LlamaCppAdapter (streaming SSE client)
│   ├── context.rs      AssembledContext — token counting + context window trimming
│   └── scheduler.rs    ProactivityScheduler — tokio interval, DeferredMessage queue
├── memory/
│   ├── mod.rs
│   ├── embedding.rs    EmbeddingService — HTTP client to :18081, tracks latency
│   ├── episodic.rs     EpisodicStore — LanceDB table, similarity search, insert
│   └── semantic.rs     SemanticStore — distillation background task
└── audio/
    ├── mod.rs
    ├── capture.rs      AudioCapture — cpal mic, VAD, energy level, dedicated thread
    ├── stt.rs          SttClient — multipart POST to parakeet :5092
    └── tts.rs          TtsClient — piper subprocess, WAV→f32, resample, cpal playback
```

---

## Port assignments

| Service | Port | Notes |
|---------|------|-------|
| llama chat | 18080 | Primary inference |
| llama embed | 18081 | Fixed to nomic-embed-text, never changes |
| parakeet STT | 5092 | **Being eliminated** — replaced by in-process ort inference |

Port range 18080–18083 was chosen to avoid collision with LM Studio (8080) and other
common local services. If 18080 is taken, change `llama_port` in `config.json`.

---

## Binary layout

### Dev (`project/binaries/` — gitignored, populated by `deno task setup`)

```
binaries/
├── llama/
│   ├── llama-server-*                 ← Windows .exe or Linux ELF
│   └── ggml / Vulkan / runtime libs
├── parakeet/
│   ├── parakeet-server-*              ← Windows: frozen exe · Linux: ~600 B bash launcher
│   └── models/                        ← optional wizard ONNX copies
└── piper/
    ├── piper-*
    ├── onnxruntime / espeak libs
    └── espeak-ng-data/
```

Linux Parakeet Python env (not under `binaries/`):

```
.cache/parakeet-tdt/                   ← gitignored; uv venv + HF model cache
```

`find_sidecar()` on Unix accepts small **executable** launchers (`> 32` bytes + exec bit).
Windows still requires files `> 1024` bytes so empty stubs are ignored.

### Production — two separate locations

**Installer bundle** (optional frozen Parakeet on Windows historically; `externalBin` is
currently **empty** — sidecars come from setup/wizard):

```
proactive-agent(.exe)
```

**Wizard / setup-downloaded** (`%APPDATA%\com.proactive.agent\binaries\` on Windows,
`~/.local/share/com.proactive.agent/binaries/` on Linux):

```
llama/
piper/
parakeet/   ← when present
```

**Why AppData / XDG data, not the exe directory:**
Program Files (and some system prefixes) need elevation to write.
The wizard downloads without elevation — user data dirs are always writable.

### `find_sidecar()` search order (release)

1. AppData / XDG `binaries/{short}/{filename}` — wizard-downloaded
2. `…/binaries/{filename}` — flat fallback
3. `{exe_dir}/{short}/{filename}` — installer-bundled (if any)
4. `{exe_dir}/{filename}` — flat installer fallback

**Library isolation:** each sidecar lives in its own subdirectory.
`make_cmd()` prepends the binary’s parent to `PATH` (Windows) or
`LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` (Unix) so native deps resolve next to the binary.

---

## Config schema

`%APPDATA%\com.proactive.agent\config.json` (release)
`data/` relative to project root (dev)

```json
{
  "chat_model": "C:/path/to/model.gguf",
  "models_dir": "C:/path/to/models",
  "db_path": "C:/path/to/memory",
  "llama_port": 18080,
  "embed_port": 18081,
  "stt_port": 5092,
  "audio_device": null,
  "embed_model": "nomic-embed-text",
  "embed_model_file": "nomic-embed-text-v1.5.Q8_0.gguf",
  "persona_prompt": "...",
  "context_window_tokens": 4096,
  "top_k_episodic": 5,
  "top_k_semantic": 5,
  "recent_turns_window": 10,
  "temperature": 0.7,
  "top_p": 0.95
}
```

Delete the file to reset all settings. The app recreates it with defaults on next launch.
Unknown fields in the JSON are silently ignored (serde default) — safe to add new fields
without breaking existing installs.

---

## Decisions already made

These were open questions at some point; they are now resolved and should not be relitigated
without a specific reason.

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Desktop framework | Tauri 2 | Native perf, Rust backend, small binary |
| Vector DB | LanceDB (Rust crate) | Embedded, no server process, Arrow native |
| Embedding model | nomic-embed-text (locked) | 768-dim, good quality, must not change |
| Inference | llama.cpp REST | OpenAI-compatible, CPU+Vulkan on same binary |
| STT runtime | Parakeet ONNX via HTTP sidecar → `ort` (planned) | Same model; ort removes Python/port |
| STT model | Parakeet TDT 0.6B | Better accuracy than Whisper on non-native speech |
| STT execution | CPU only | GPU reserved exclusively for LLM — headroom for larger models |
| JS toolchain | Deno preferred; npm supported | `deno.json` + Node-compatible `package.json` |
| TTS | Piper (subprocess) | Offline, natural quality, espeak-ng phonemization complex to replace |
| TTS execution | CPU only | Same rationale — no GPU competition with LLM |
| Audio resampling | rubato SincFixedIn | Device-rate-aware, adapts to any mic without code changes |
| Audio I/O | cpal | Cross-platform, direct PCM access |
| Proactivity | LLM-emitted `<defer>` tags | Model drives scheduling, no hardcoded rules |
| Memory isolation | Two LanceDB tables (episodic + semantic) | Different update frequency and noise level |
| Persona | Fixed system prompt, never diluted by retrieval | Memory injected beneath it as context |
| Port range | 18080–18083 | Avoids LM Studio (8080) collision |
| Context eviction | Trim oldest episodic first | Semantic memory is more stable; protect it |
| Whisper | Retired permanently | Accuracy unacceptable on non-native accents — do not revisit |

---

## Known technical debt

| Item | Location | Impact |
|------|----------|--------|
| `espeak-ng-data/` not in production installer | `tauri.conf.json` | TTS silent in MSI install |
| `ggml-cpu-*.dll` not bundled | `tauri.conf.json` | CPU fallback uses slow reference kernels |
| Linux STT still needs Python (managed venv) | `.cache/parakeet-tdt/` | Interim until ort; not in Rust crate graph |
| Windows Parakeet has no public release URL | Distribution | Frozen sidecar must be supplied manually |
| No typed IPC bindings (`tauri-specta`) | Frontend/backend boundary | Runtime errors if command signatures drift |
| `stt_port` hardcoded in upstream Parakeet | config vs sidecar | Cannot reassign port without changing server env/code |
| Linear interpolation resampler | `audio/tts.rs` | Good enough; sinc would be higher quality |
| No retry logic in STT client | `audio/stt.rs` | Transient failure can drop a transcript |

See `ROADMAP.md` for what is actively planned.

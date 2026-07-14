# Proactive Agent — Roadmap

**Current state:** Chat, streaming, memory, voice in/out, debug panel working on Linux
(Deno toolchain) and Windows. Wizard / `deno task setup` downloads llama + piper;
Linux additionally installs a Parakeet launcher that the app auto-spawns. Rubato sinc
resampling in the STT pipeline. Long-term STT target remains in-process `ort`.

---

## ✅ Done

| Item | Notes |
|------|-------|
| Chat inference | CPU binary + Vulkan libs, port 18080 |
| Streaming responses | SSE token streaming, blinking cursor |
| Vector memory (episodic) | LanceDB + nomic-embed-text |
| Semantic distillation | LLM-based, runs every 10 min |
| Context overflow handling | Trims oldest episodic first |
| Model parameters | Temperature/top-p/context sliders |
| Chat history persistence | localStorage, survives restarts |
| Loading state | Green/blue status dot in header |
| Proactive `<defer>` tags | Scheduler built + tested |
| Voice input (STT) | Parakeet TDT via HTTP, mono downmix, normalization |
| Voice output (TTS) | Piper subprocess, resampled + stereo upmix |
| Debug panel | Event log, context inspector, sidecar health, scheduler |
| System requirements check | OS-aware (VCRedist Windows-only; Vulkan; llama probe) |
| Model hot-swap | Works without restart |
| Memory reset | Wipes LanceDB + recent turns, requires typing RESET |
| Graceful shutdown | All sidecars killed on app exit, libs released |
| App icon | SVG source → all sizes via `deno task tauri icon` |
| TTS speed fix | WAV resampled to device rate + mono→stereo upmix |
| Production build | MSI (96 MB) + NSIS (74 MB) installers |
| Unit tests (TTS) | 13 tests: `wav_to_f32`, `resample`, `clean_for_speech` |
| Wizard binary downloads | llama-server + piper downloaded by wizard on first run |
| Rubato STT resampling | `SincFixedIn` 48kHz→16kHz, spawn_blocking, device-rate-aware |
| Constants consolidation | `constants.rs` — all URLs, filenames, timeouts in one place |
| Sidecar health polish | Amber "loading model" state, 2-poll debounce |
| Episodic role labels | `User:`/`Assistant:` prefix in retrieved memories, typed `Role` enum |
| Security hardening | CSP, removed shell permissions, path validation, input limits |
| Deno toolchain | `deno.json` + setup/frontend runners; npm still supported |
| Linux sidecar fetch | `fetch-sidecars-linux.sh` — llama tarball, Vulkan `.so` soname links |
| Linux Parakeet auto-start | Managed CPU Python env + small launcher; app spawn + mic warm-up wait |
| Empty `externalBin` | No compile-time bundled sidecars; wizard/setup populates `binaries/` |

---

## 🏗️ Next — STT migration to ort (in-process, CPU only)

**Decision made:** Replace the Parakeet HTTP sidecar (Windows PyInstaller **or** Linux
managed Python launcher) with native Rust `ort` inference.
Full plan: `STT_ORT_MIGRATION.md`

### Architecture decision (locked)

```
GPU (VRAM)  →  LLM only. Reserved entirely. Growing headroom for larger models.
CPU         →  STT via ort (Parakeet ONNX, in-process)
               TTS via Piper subprocess (unchanged)
               Embeddings via llama-server CPU path
```

### Current interim (works today)
- **Windows:** frozen `parakeet-server` when present; port 5092
- **Linux:** `deno task setup` writes a small executable launcher under
  `binaries/parakeet/`; Tauri spawns it on startup (venv in `.cache/parakeet-tdt/`)
- Mic fails fast (with warm-up wait) if `/healthz` never comes up

### What ort unlocks
- Python dependency eliminated entirely (including the Linux managed venv)
- macOS unblocked — ONNX model is cross-platform, no rebuild needed
- Thin installer: no frozen Python binary / no `.cache/parakeet-tdt`
- Port 5092 gone — direct function call, no network overhead
- GPU fully free for LLM headroom

### Implementation steps (in order)

- [ ] Add `ort` + `rustfft` to `Cargo.toml`
- [ ] Implement `log_mel_spectrogram()` in Rust, verify against Python server output
- [ ] Implement greedy CTC decoder using downloaded tokens.txt
- [ ] Rewrite `audio/stt.rs` — `SttClient` holds `ort::Session` instead of `reqwest::Client`
- [ ] Wire into `run_stt_loop` via `spawn_blocking`
- [ ] Initialise `SttClient` in app setup, manage as app state
- [x] Remove parakeet from `tauri.conf.json` externalBin *(done — array is empty)*
- [ ] Remove parakeet from monitor health-check loop
- [ ] Remove parakeet row from `SidecarHealth.tsx`
- [ ] Remove Linux `run-parakeet-linux.sh` / launcher path from setup
- [ ] Test: English, non-native accent — confirm same or better accuracy
- [ ] Confirm zero GPU memory used during STT

---

## 🔜 Near-term follow-on

### GPU layer offload slider

llama-server already supports `-ngl N`. A slider in the Models tab exposes it:
`0 layers ← ——————— → all layers (current default: 999)`

- `ngl=0` → full CPU, 0 VRAM — any machine can run any model
- `ngl=N` → hybrid — first N layers GPU, rest CPU
- `ngl=999` → all layers GPU (current)

LM Studio calls this "GPU Offload". Implementation: one config field + one slider +
one changed CLI arg to llama-server. No model changes.

**Why it matters:** Once Parakeet is off the GPU, the LLM is the only VRAM consumer.
This slider lets users tune speed vs model size headroom freely.

---

## 🧹 Pending cleanup

| Item | Notes |
|------|-------|
| VCRedist SHA256 placeholder | Replace `c760c594...` in `constants.rs` with real hash |
| `capture error` still `eprintln!` | Should route to debug event log |
| STT VAD diagnostic logs | Make debug-only (`#[cfg(debug_assertions)]`) |
| `ggml-cpu-*.dll` not in installer | CPU fallback slow without them |
| `libomp140.x86_64.dll` not in installer | OpenMP parallelism for llama-server |
| TTS rubato upgrade | Linear interpolation works; could be upgraded for consistency |
| `tauri-specta` typed IPC | Type-safe invoke/event at the Rust/frontend boundary |

---

## ⬜ Later

### macOS support
Unblocked by ort migration. After that:
- `binary_store.rs` already has macOS asset patterns for piper + llama
- Test on M1/M2

### Vitest frontend tests
Low priority while UI is prototype stage.

---

## Docs

| File | Purpose |
|------|---------|
| `ARCHITECTURE.md` | Current system — ports, binaries, decisions |
| `SETUP.md` | Install and run |
| `STT_ORT_MIGRATION.md` | Next STT migration (HTTP → ort) |
| `WORK_LOG.md` | Bug history and rationale |
| `CONTRIBUTING.md` | Branch → test → merge |
| `SUPERVISOR.md` | AI review checklist |


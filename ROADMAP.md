# Proactive Agent — Roadmap

**Current state:** Chat, streaming, memory, voice input (STT), voice output (TTS), debug panel all working.

---

## ✅ Done

| Item | Notes |
|------|-------|
| Chat inference | CPU binary + Vulkan DLLs, port 18080 |
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
| System requirements check | Auto-installs VCRedist |
| Model hot-swap | Works without restart |
| Memory reset | Wipes LanceDB + recent turns, requires typing RESET |
| Graceful shutdown | All sidecars killed on app exit, DLLs released |
| Git hygiene | .gitattributes, CRLF warnings gone |
| App icon | SVG source → all sizes via `npm run tauri icon` |
| TTS test button | Debug panel → dev tools → "🔊 test voice" |

---

## ⬜ Remaining

### Verify TTS speed (first thing next session)
The latest fix (sample rate + channel upmix) was just committed. Confirm voice plays
at natural speed. Expected [AUDIO] log: `TTS: N KB @ 22050Hz 1ch — playing`.

### Production build
- `npm run tauri build` → produces installer at `src-tauri/target/release/bundle/msi/`
- Debug commands already gated behind `#[cfg(debug_assertions)]`
- Icon already set up from app-icon.svg
- Verify sidecar bundling works in release package

### STT accuracy (low priority — model limitation)
Parakeet TDT 0.6B struggles with non-native accents. Options if needed:
- Parakeet 1.1B (larger, better)
- Migrate to `ort` Rust crate for in-process inference (see STT_MIGRATION.md)

### macOS support
- Run `scripts/fetch-sidecars-macos.sh`
- Parakeet binary needs macOS build
- Piper has `piper_macos_aarch64.tar.gz` in releases
- Test on M1/M2

---

## Architecture quick reference

### Ports
| Service | Port |
|---------|------|
| llama chat | 18080 |
| llama embed | 18081 |
| parakeet STT | **5092** (hardcoded in parakeet binary, cannot change) |
| kokoro (unused) | 18083 |

### Binary sources
| Binary | Source | Notes |
|--------|--------|-------|
| llama-server | ggerganov/llama.cpp CPU build | + Vulkan DLLs for GPU |
| parakeet-server | groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai | PyInstaller frozen |
| piper | rhasspy/piper v2023.11.14-2 | piper_windows_amd64.zip |
| whisper-server | retired | replaced by Parakeet |

### TTS pipeline detail
```
response text
→ clean_for_speech() strips markdown/code/URLs
→ Piper subprocess (stdin=text, --model=en_US-lessac-medium.onnx)
→ WAV file @ 22050Hz mono
→ wav_to_f32() parses header (bytes 22-27) → (samples, rate, channels)
→ resample() linear interp 22050→device_rate
→ upmix mono→stereo (duplicate L+R)
→ cpal playback
```

### STT pipeline detail
```
mic @ 48000Hz stereo
→ VAD threshold 0.005 RMS → capture frames
→ 800ms silence → transcribe
→ downmix stereo→mono
→ normalize to 0.7 peak amplitude
→ pcm_to_wav() encode
→ POST /v1/audio/transcriptions @ parakeet:5092
→ clean_transcript() filter hallucinations
→ emit voice_transcript Tauri event
→ sendMessage()
```

### Key config file
`%APPDATA%\com.proactive.agent\config.json` — delete to reset all settings to defaults.

### Parakeet binary notes
- Port 5092 hardcoded in app.py line 4 (cannot be changed via CLI)
- On first run, server downloads ONNX model from HuggingFace to its cache (~600MB)
- Browser tab auto-open was patched out before building
- Binary built with: `pyinstaller --onefile --collect-all onnx-asr --collect-data onnx_asr --add-data dist-info:dist-info app.py`

### Piper binary notes
- Downloaded from: `https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip`
- Requires alongside exe: `onnxruntime.dll`, `espeak-ng.dll`, `piper_phonemize.dll`, `espeak-ng-data/`
- Voice model: `models/tts/en_US-lessac-medium.onnx` + `.onnx.json`
- Voices library: https://github.com/rhasspy/piper-voices

---

## ♻️ Self-recovery after deleting `binaries/`

**Goal:** deleting `binaries/` and restarting should trigger full re-setup with no terminal commands.

### Current status per component

| Component | Recoverable without terminal? | How |
|-----------|------------------------------|-----|
| nomic-embed-text model | ✅ | SetupWizard downloads it |
| Piper voice model | ✅ | SetupWizard downloads it |
| Parakeet ONNX model | ✅ | Parakeet server downloads on first run |
| **llama-server binary** | ❌ | Requires `npm run setup` (dev tool) |
| **piper binary** | ❌ | Requires `npm run setup` (dev tool) |
| **parakeet-server binary** | ❌ | Requires manual PyInstaller build |

### What needs to be added to SetupWizard

**llama-server binary** — download CPU build + Vulkan DLLs from GitHub releases:
```
GET https://api.github.com/repos/ggerganov/llama.cpp/releases/latest
→ Download piper_windows_amd64.zip asset
→ Extract llama-server.exe → binaries/llama/llama-server-x86_64-pc-windows-msvc.exe
→ Download Vulkan zip → extract all DLLs to binaries/llama/
```

**piper binary** — download from GitHub releases:
```
GET https://api.github.com/repos/rhasspy/piper/releases/latest
→ Download piper_windows_amd64.zip
→ Extract piper.exe + DLLs + espeak-ng-data/ → binaries/piper/
```

**parakeet-server binary** — hardest. Options:
- a) Host the frozen binary on a private GitHub release and download it
- b) Ship it inside the Tauri installer bundle (via `externalBin`) — best option for distribution
- c) Accept that rebuilding it requires Python (developer scenario only)

### For production (installer)
When built with `npm run tauri build`, Tauri bundles ALL `externalBin` entries into
the installer package. A user installing via the `.msi` gets all binaries included —
they never see `binaries/` as a separate folder. Self-recovery doesn't apply.

Self-recovery only matters for the **developer workflow** (working from source).
For that: `npm run setup` recovers llama + piper. Parakeet binary needs a separate step.

### Roadmap item: add binary downloads to SetupWizard
Add a `download_required_binaries()` Tauri command that:
1. Checks if `llama-server` is present and functional (`/health` returns 200)
2. If not: downloads CPU build + Vulkan DLLs from ggerganov/llama.cpp latest
3. Checks if `piper` is present
4. If not: downloads piper_windows_amd64.zip from rhasspy/piper latest
5. Shows progress bars (same infrastructure as model downloads)
6. SetupWizard step 0 (before models): "Downloading required tools"

This would make `npm run setup` completely obsolete for both developers and users.

---

## 🧹 Cleanup — obsolete code to remove

These are leftovers from replaced/retired features. Low risk, no functionality depends on them.

### Binaries (safe to delete)
- `binaries/whisper/` — entire directory. Whisper retired, replaced by Parakeet.
- `binaries/kokoro/` — entire directory. sherpa-onnx replaced by Piper subprocess.
- `binaries/kokoro-server-x86_64-pc-windows-msvc.exe` — root placeholder, 0 bytes.
- `binaries/whisper-server-x86_64-pc-windows-msvc.exe` — root placeholder, 0 bytes.
- `binaries/whisper.dll` — DLL from old whisper layout.
- Root-level DLLs in `binaries/` (CONCRT140, MSVCP140, SDL2, ggml-*.dll etc.) — these
  belong in `binaries/llama/` and are already there. Root copies are orphaned.

### Rust — `config.rs`
- `kokoro_port: u16` field — TTS now uses Piper subprocess, no HTTP server, no port needed.
- Remove from `AppConfig` struct and `with_data_dir()` default.

### Rust — `lib.rs`
- `kokoro_port` variable in `spawn_sidecars()` (line ~367).
- The whole TTS sidecar spawn block that checks `find_sidecar("kokoro-server")` —
  Piper is a subprocess call from `TtsClient`, not a long-running server.
  This block logs "TTS (sherpa-onnx) ready" which is misleading.

### Rust — `monitor.rs`
- `("kokoro", kokoro_port)` entry in the sidecar health check loop —
  kokoro has no HTTP server to ping. Remove to stop the red dot and timeout noise.

### Rust — `commands.rs`
- `root.join("whisper")` in `diagnose_chat_server` (line ~426) — dead path.

### Frontend — `SidecarHealth.tsx`
- `'kokoro'` in the `SIDECARS` array — no server to monitor. Remove the row.

### Scripts
- `scripts/kokoro_server.py` — Python Kokoro server, replaced by Piper.
- `scripts/build-kokoro-exe.ps1` — builds the Python Kokoro binary, no longer needed.
- `app-icon.png`, `UsersRothWorkproactive-aiapp-icon.png` — leftover PNG attempts,
  `app-icon.svg` is the canonical source now.
- `scripts/make_icon.py` — one-off script, no longer needed.

### Models
- `models/ggml-medium.en.bin` — old Whisper medium model, ~1.5 GB. Delete to free space.
- `models/ggml-small.en.bin` — old Whisper small model, ~466 MB. Delete to free space.
- `models/ggml-base.en.bin` — old Whisper base model, ~142 MB. Delete to free space.

### tauri.conf.json
- `"../binaries/kokoro-server"` in `externalBin` — Piper is a subprocess, not a sidecar.
  Replace with `"../binaries/piper/piper"` (or remove if already updated).
</content>
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
</content>
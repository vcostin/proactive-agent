# STT Migration: Whisper → Parakeet TDT

**Project:** Proactive Agent | **Date:** May 2026

Surgical replacement of whisper.cpp with NVIDIA Parakeet TDT 0.6B v3 via ONNX Runtime.
CPU-only, no Python at runtime, no CUDA, no ROCm. Everything outside `audio/stt.rs` is untouched.

> **STANDALONE CONSTRAINT:** The app must be self-sufficient after a double-click install.
> No Node, npm, cargo, Python, or dev tools at runtime. All STT dependencies — binary and
> model files — must be either bundled at build time or downloaded via the in-app SetupWizard.
> There is no `npm run setup`. There is no manual step.

---

## Why Parakeet

| Criterion | Result |
|-----------|--------|
| Romanian + Russian support | ✅ Both in the 25 supported EU languages, auto-detected |
| CPU-only (no CUDA/ROCm) | ✅ ONNX INT8 build runs on CPU — AMD GPU not required |
| Speed vs Whisper Large v3 | ✅ Faster on CPU; beats Whisper Large v3 Turbo in benchmarks |
| Wire format compatibility | ✅ Same multipart POST → `{ "text": "..." }` — SttClient barely changes |
| License | ✅ CC-BY-4.0 — commercial use permitted |
| Standalone-friendly | ✅ Frozen binary + ONNX files — no Python runtime needed at runtime |

whisper.cpp is explicitly retired. Do not re-introduce it.

---

## What to Delete

### Binaries
- Delete `binaries/whisper/` and everything inside it.

### `tauri.conf.json` — externalBin
Remove:
```
"../binaries/whisper/whisper-server-x86_64-pc-windows-msvc"
"../binaries/whisper/whisper-server-aarch64-apple-darwin"
```
Add:
```
"../binaries/parakeet/parakeet-server-x86_64-pc-windows-msvc"
"../binaries/parakeet/parakeet-server-aarch64-apple-darwin"
```

### `config.rs`
Remove `whisper_model_path: PathBuf` from `AppConfig`. Parakeet locates its model
relative to its own binary — no user-facing path needed.

Rename if present: `whisper_port` → `stt_port` (value stays `18081`).

### `scripts/fetch-sidecars-windows.ps1`
Remove the whisper download block entirely. The SetupWizard owns all downloads now (see Section: Setup Wizard).

---

## Rust Changes

### `main.rs` — sidecar spawn

```rust
// BEFORE
let (_rx, _child) = tauri::api::process::Command::new_sidecar("whisper-server")
    .args(["-H", "127.0.0.1", "-p", "18081", "--model", &whisper_model_path])
    .spawn()?;

// AFTER
let (_rx, _child) = tauri::api::process::Command::new_sidecar("parakeet-server")
    .args(["--host", "127.0.0.1", "--port", "18081"])
    .spawn()?;
```

Parakeet loads its model from a `models/` directory co-located with the binary at startup.
No `--model` flag. See Directory Layout below.

### `audio/stt.rs` — endpoint

```rust
// BEFORE
const STT_ENDPOINT: &str = "http://127.0.0.1:18081/inference";

// AFTER
const STT_ENDPOINT: &str = "http://127.0.0.1:18081/v1/audio/transcriptions";
```

Request body (multipart), response struct, and health check path (`/health`) are identical.
No other changes to `SttClient`.

### `monitor.rs` — sidecar label

Update the display name from `"Whisper"` to `"Parakeet STT"` wherever it appears in
`SidecarHealth` events.

---

## Sidecar Binary

### Directory layout (both dev and release)

```
binaries/
  parakeet/
    parakeet-server-x86_64-pc-windows-msvc.exe   ← frozen binary
    parakeet-server-aarch64-apple-darwin          ← frozen binary
    models/
      parakeet-tdt-0.6b-v3.onnx
      parakeet-tdt-0.6b-v3-tokens.txt
```

In release builds `binaries_dir()` must resolve relative to `current_exe().parent()`,
not `current_dir()`. This is the same fix already flagged for llama.cpp — apply identically.

### Building the frozen binary

Source: https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai

This is done once at build/release time, not by the end user.

```powershell
# On a dev machine with Python 3.10+
git clone https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai
cd parakeet-tdt-0.6b-v3-fastapi-openai
pip install -r requirements.txt
pip install pyinstaller

# Windows
pyinstaller --onefile main.py --name parakeet-server-x86_64-pc-windows-msvc

# macOS (run on macOS)
pyinstaller --onefile main.py --name parakeet-server-aarch64-apple-darwin

# Copy output
cp dist/parakeet-server-*.exe ../binaries/parakeet/
```

The ONNX model files are downloaded on first run by the server itself, or you can
pre-download and bundle them alongside the binary (recommended for air-gapped installs).

### Future path: native Rust via `ort` crate

If you want to eliminate the sidecar process entirely and run Parakeet in-process:

```toml
# Cargo.toml
ort = { version = "2", features = ["load-dynamic"] }
```

Load the ONNX model directly in `audio/stt.rs`, preprocess audio to a 16kHz mono float
tensor, run inference. Removes the HTTP round-trip and the frozen Python binary entirely.
On AMD/Windows, `ort` supports DirectML as a GPU backend — no ROCm needed if you ever
want hardware acceleration.

Recommended: validate Parakeet works for your languages with the sidecar first, then
migrate to `ort` if the overhead matters.

---

## Setup Wizard — Standalone Integration

The SetupWizard is the only place where STT assets are acquired at runtime.
No terminal step. No script. Just the wizard.

### Remove
- Whisper model download step and its progress bar component
- `whisper_model_path` from the config fields the wizard writes

### Add

New wizard step: **"Speech Recognition Model"**

- Display: "Downloading Parakeet STT model (~600 MB)"
- Downloads two files from Hugging Face (nvidia/parakeet-tdt-0.6b-v3 ONNX export):
  - `parakeet-tdt-0.6b-v3.onnx`
  - `parakeet-tdt-0.6b-v3-tokens.txt`
- Target: `binaries_dir()/parakeet/models/`
- Uses the same progress bar + download infrastructure already built for the chat model
- On completion, verify the parakeet-server binary starts and `/health` returns 200 before
  advancing the wizard

> Verify the Hugging Face download URLs before hardcoding — the ONNX export comes from
> the groxaxo repo (see Sidecar Binary section), not the official nvidia/ HF page directly.

---

## ARCHITECTURE.md — Required Updates

### Hard Constraints — replace STT line
```
# REMOVE:
- **whisper.cpp** as the STT sidecar

# ADD:
- **Parakeet TDT 0.6B v3 (ONNX)** as the STT sidecar — frozen binary sidecar or
  in-process via `ort` Rust crate. Supports Romanian, Russian, and 23 other EU
  languages with automatic detection. whisper.cpp is explicitly retired.
  Do not re-introduce it.
```

### Audio Pipeline diagram — replace STT line
```
# REMOVE:
     → whisper.cpp sidecar (STT)

# ADD:
     → Parakeet TDT sidecar (STT) — ONNX, CPU, port 18081
```

### Explicit Decisions — replace STT line
```
# REMOVE:
- STT: whisper.cpp

# ADD:
- STT: Parakeet TDT 0.6B v3 (ONNX) — whisper.cpp retired, do not revisit
```

### Known Implementation Risks — add entry
```
**Parakeet ONNX model path (release builds)**
Model files must be co-located with the parakeet-server binary under `models/`.
In release mode, `binaries_dir()` must use `current_exe().parent()` — same fix
already documented for llama.cpp. Apply identically.
```

---

## ROADMAP.md — Required Updates

```markdown
### ~~2. Whisper-server stays alive~~ ❌ RETIRED
Replaced by Parakeet TDT 0.6B v3. See STT_MIGRATION.md.

### 2b. Parakeet STT sidecar ⬜ TODO
- [ ] Build frozen parakeet-server binary (Windows + macOS)
- [ ] Place in binaries/parakeet/ with models/ subdirectory
- [ ] Update tauri.conf.json externalBin
- [ ] Update SttClient endpoint to /v1/audio/transcriptions
- [ ] Update SetupWizard: remove whisper step, add Parakeet download step
- [ ] Verify wizard downloads to correct binaries_dir() in release mode
- [ ] Test Romanian speech → transcript
- [ ] Test Russian speech → transcript
- [ ] Green dot in SidecarHealth debug panel
```

---

## Testing Checklist

- [ ] `cargo check` passes, zero whisper references in source
- [ ] `curl http://127.0.0.1:18081/health` → 200
- [ ] English 5s utterance → accurate transcript, < 2s latency on 7950X3D
- [ ] Romanian utterance → accurate transcript, language auto-detected
- [ ] Russian utterance → accurate transcript, language auto-detected
- [ ] Voice toggle → mic → transcript → LLM → response displayed in chat
- [ ] SidecarHealth debug panel shows "Parakeet STT" with green dot
- [ ] SetupWizard completes without any terminal step
- [ ] Release build: model files resolve correctly from `current_exe().parent()`
- [ ] No whisper binary or reference survives in the release package

---

## Quick Reference

| Item | Value |
|------|-------|
| Model | nvidia/parakeet-tdt-0.6b-v3 |
| Sidecar source | github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai |
| STT port | 18081 (unchanged) |
| Health endpoint | `GET /health` |
| Transcription endpoint | `POST /v1/audio/transcriptions` |
| Request | `multipart/form-data`, field `file` (WAV) |
| Response | `{ "text": "..." }` |
| Model size (ONNX INT8) | ~600 MB |
| Languages | 25 EU languages incl. Romanian, Russian, Ukrainian |
| License | CC-BY-4.0 |

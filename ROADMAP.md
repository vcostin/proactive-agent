# Proactive Agent — Roadmap

**Current state:** chat + streaming + memory + voice input + debug panel all working.
Model parameters exposed in UI. Semantic distillation running in background.

---

## P1 — Core gaps

### ~~1. Voice input wiring~~ ✅ DONE
AudioCapture → STT loop → `voice_transcript` event → `sendMessage`. Mic toggle in ChatWindow.

### 2. Whisper-server stays alive ⚠️ IN PROGRESS
Whisper loads model successfully but exits after init. Added `-H 127.0.0.1 -t 4` flags.
- Check `[ADAPTER]` log after restart to see exit code
- If still failing: try `--host` vs `-H`, or download a different whisper.cpp build

### 3. Kokoro TTS ⬜
Placeholder binary. Best options:
- **a) sherpa-onnx** — pre-built Windows binary, ONNX TTS, no Python required, fast.
  Has an HTTP server mode with OpenAI-compatible `/v1/audio/speech`.
- b) `scripts/build-kokoro-exe.ps1` — PyInstaller (requires Python 3.10+)
- c) piper-tts — CLI only, needs a wrapper server

### ~~4. Streaming chat responses~~ ✅ DONE
SSE token streaming with blinking cursor bubble.

### ~~5. Chat history persistence~~ ✅ DONE
localStorage, survives restarts, 200-message cap, clear button.

---

## P2 — Quality of life

### ~~6. Loading state while model warms up~~ ✅ DONE
Blue spinner banner, disabled send button, auto-enables when health check passes.

### ~~7. Semantic memory distillation~~ ✅ DONE
Background task every 10 min. LLM extracts durable facts from episodic turns.
Auto-classifies into preference/project/skill/habit categories. Logs to [MEMORY].

### ~~8. Context window overflow handling~~ ✅ DONE
Trims oldest episodic entries first, then recent turns. Never hard-fails.

### ~~9. Model parameter controls~~ ✅ DONE
Temperature, Top-P, context window sliders in Models tab. Persisted to config.json.
Passed to every inference call via `GenParams`.

### 10. `<defer>` proactivity end-to-end test ⬜
Scheduler is fully built and tested via "Fire Now" in debug panel.
Still needed:
- Verify the model actually emits `<defer>` tags in response
- Make the persona prompt instruction more explicit/prominent
- Confirm the proactive message appears in chat as the orange ◈ bubble

---

## P3 — Distribution

### 11. Real app icon ⬜
`icons/icon.ico` is a 1×1 teal placeholder pixel. Replace before any release.

### 12. macOS support ⬜
- Run `scripts/fetch-sidecars-macos.sh` and verify on M1/M2
- Test port 18080 availability
- Verify aarch64-apple-darwin binary naming

### 13. Production build ⬜
- Gate debug commands behind `#[cfg(debug_assertions)]`
  (`diagnose_chat_server`, `open_llama_diagnostic`)
- Test `npm run tauri build` → produces installer
- Verify sidecar bundling in release package

### 14. Git hygiene ⬜
- Add `.gitattributes`: `* text=auto eol=lf` to stop the CRLF warnings on every commit
- Keep `Cargo.lock` committed (it's a binary app, not a library)

### 15. Self-sufficient distribution (P3 — do before any release) ⬜
The installed app must run with a double-click. No Node, npm, cargo, Python, or dev
tools should be required at runtime. Most pieces are already in place:

**Already self-sufficient:**
- ✅ In-app model download (SetupWizard)
- ✅ VCRedist auto-install (SystemRequirements)
- ✅ sidecar binaries declared via `externalBin` (Tauri bundles them)
- ✅ First-run wizard replaces any setup scripts

**Still needed:**
- `binaries_dir()` in release mode should use `std::env::current_exe().parent()`
  not `current_dir()` (different paths in a bundled app)
- `tauri.conf.json` `externalBin` paths need to work for both dev (`../binaries/X`)
  and release (bundled alongside exe) — test with `npm run tauri build`
- Kokoro/TTS: must be a pre-built binary (sherpa-onnx) — no Python runtime
- Whisper: must stay alive without manual restarts
- `data/` directory: in release use OS app data dir, not project root
  (already done for config.json, confirm for LanceDB and models)
- Remove the `npm run setup` step entirely — user should get everything
  from the in-app wizard, not a terminal script

---

## Architecture items status

| Item | Status |
|------|--------|
| Chat inference | ✅ working (CPU binary + Vulkan DLLs, port 18080) |
| Vector memory (episodic) | ✅ working (LanceDB + nomic-embed-text) |
| Semantic distillation | ✅ implemented, runs every 10 min |
| Context assembly | ✅ working, overflow trimming added |
| Proactive `<defer>` tags | ✅ scheduler built, needs model output testing |
| Voice input (mic → STT → chat) | ✅ wired, whisper exit issue under investigation |
| Kokoro TTS | ⬜ placeholder binary |
| Streaming responses | ✅ working |
| Debug panel | ✅ fully working |
| System requirements check | ✅ auto-installs VCRedist |
| Model hot-swap | ✅ working |
| Model parameters | ✅ temperature/top-p/context in UI |
| Chat history persistence | ✅ localStorage |
| macOS Metal backend | ⬜ fetch script written, untested |
| Fine-tune management | ⬜ placeholder row in Models tab |

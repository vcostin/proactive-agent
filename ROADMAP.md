# Proactive Agent — Roadmap

**Current state:** Chat, streaming, memory, voice input, debug panel, STT (Parakeet), proactivity all working.

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
| Loading state | Status dot in header |
| Proactive `<defer>` tags | Scheduler built + tested |
| Voice input (STT) | Parakeet TDT via HTTP, mono downmix, normalization |
| Debug panel | Event log, context inspector, sidecar health, scheduler |
| System requirements check | Auto-installs VCRedist |
| Model hot-swap | Works without restart |
| Memory reset | Wipes LanceDB + recent turns, requires typing RESET |
| Graceful shutdown | All sidecars killed on app exit, DLLs released |
| Git hygiene | .gitattributes, CRLF warnings gone |

---

## ⬜ Remaining

### P1 — Still incomplete

**STT accuracy tuning**
Parakeet works but accuracy needs improvement for non-native accents.
Options (pick one):
- a) Parakeet 1.1B — larger model, likely better
- b) Migrate to `ort` Rust crate (STT_MIGRATION.md §"Future path") — eliminate Python binary, run ONNX in-process, full control over preprocessing
- c) Accept current quality, use keyboard as primary

**TTS (voice output)**
sherpa-onnx subprocess mode is implemented but untested end-to-end.
The `TtsClient.speak()` is never called yet — need to wire it to the assistant response pipeline.

---

### P3 — Distribution (do before any public release)

**Real app icon**
`icons/icon.ico` is a 1×1 placeholder pixel.

**Production build**
- Gate `diagnose_chat_server` / `open_llama_diagnostic` behind `#[cfg(debug_assertions)]`
- Test `npm run tauri build` → installer
- Verify sidecar bundling works in release package
- Confirm `binaries_dir()` resolves correctly from `current_exe().parent()` in release

**macOS support**
- Run `scripts/fetch-sidecars-macos.sh` and test on M1/M2
- Verify aarch64-apple-darwin binary naming

---

## Architecture items status

| Item | Status |
|------|--------|
| Chat inference | ✅ |
| Vector memory | ✅ |
| Semantic distillation | ✅ |
| Streaming | ✅ |
| Proactive scheduler | ✅ |
| Voice input (STT) | ✅ working, accuracy improvable |
| TTS (voice output) | ⬜ wired but not connected to response pipeline |
| Debug panel | ✅ |
| Memory reset | ✅ |
| Graceful shutdown | ✅ |
| Production build | ⬜ |
| Real app icon | ⬜ |
| macOS | ⬜ fetch script exists, untested |

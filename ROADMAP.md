# Proactive Agent — Roadmap

Current state: chat works, memory works, debug panel works, sidecar management works.

---

## P1 — Core gaps (do these first)

### 1. Voice input wiring
`AudioCapture` and `SttClient` are fully built in Phase 4 but not connected to the UI.

- Start audio capture when user clicks the 🎙 toggle in `ChatWindow`
- Run `audio::run_stt_loop` in a background tokio task
- Route transcript into `send_message` instead of keyboard input
- Show VAD energy bar live via `useSystemStatus` (AudioState already has `energy_level`)

### 2. Whisper-server port binding
Whisper loads the model and uses GPU but exits after load. Needs investigation:
- Check if `-p 18082` is the correct argument format for this whisper build
- Try `--port 18082` vs `-p 18082` vs other variants
- May need a different whisper.cpp release

### 3. Kokoro TTS
Placeholder binary only. Options in order of effort:
- a) `scripts/build-kokoro-exe.ps1` — compile `kokoro_server.py` via PyInstaller (Python required)
- b) Find a pre-built kokoro-cpp binary with OpenAI-compatible `/v1/audio/speech` endpoint
- c) Use `piper-tts` as a drop-in replacement (ONNX, faster, no Python)

### 4. Streaming chat responses
Currently waits for full response. Add streaming:
- Set `stream: true` in `ChatRequest`
- Parse Server-Sent Events in the adapter
- Push tokens to the frontend as they arrive
- Shows intermediate tokens in the message bubble

### 5. Chat history persistence
Messages live only in React state — lost on restart.
- Add `save_conversation` Tauri command (writes turns to `data/history.json`)
- Load on startup and populate `useChat` state
- Add "New conversation" button to clear

---

## P2 — Quality of life

### 6. Loading state while model warms up
After llama-server starts it takes 5-30s to be ready. During that window, messages fail silently.
- Poll `/health` in the adapter before sending, retry with backoff
- Show "Loading model…" spinner in the chat header when llama :18080 is unreachable

### 7. Semantic memory distillation
`SemanticStore.distill()` is a stub. Implement:
- Background tokio task running every N minutes
- Reads recent episodic entries, calls LLM to extract durable facts
- Writes to semantic collection
- Update `MemoryStats.last_distillation` timestamp

### 8. Context window overflow handling
When total tokens exceed `context_window_tokens`, the request will fail.
- Detect overflow before sending (use `AssembledContext.total_tokens()`)
- Strategy: drop oldest episodic entries first, then summarise recent turns

### 9. Model parameter controls
Add sliders/inputs to the Models tab or a settings panel:
- Temperature (default 0.8)
- Top-P / Top-K
- Context window size
- GPU layers (`-ngl`)

### 10. `<defer>` proactivity end-to-end test
The scheduler is fully built. Test with a model that actually emits `<defer>` tags:
- Add the defer format to the persona prompt more explicitly
- Use the "Fire Now" button in the debug Scheduler panel to test firing
- Verify the proactive message appears in chat as a special bubble

---

## P3 — Distribution

### 11. Real app icon
`icons/icon.ico` is a 1×1 placeholder pixel. Replace with a real icon.

### 12. macOS support
- Run `scripts/fetch-sidecars-macos.sh` and verify on macOS M1
- Test port 18080 availability on macOS
- Check binary naming conventions for aarch64-apple-darwin

### 13. Production build
- Remove diagnostic/debug commands (`diagnose_chat_server`, `open_llama_diagnostic`)
  before shipping, or gate them behind `#[cfg(debug_assertions)]`
- Test `npm run tauri build` — produces a real installer
- Verify sidecar bundling in the release package

### 14. Git hygiene
- Add `.gitattributes` with `* text=auto eol=lf` to stop the CRLF warnings
- Consider adding `Cargo.lock` to `.gitignore` or keeping it — decide and commit

---

## Architecture items still open (from ARCHITECTURE.md)

| Item | Status |
|------|--------|
| Proactive `<defer>` tags | Scheduler built, model cooperation needed |
| Semantic distillation | Stub — needs LLM extraction impl |
| Voice pipeline (full loop) | Parts built, not wired end-to-end |
| Fine-tune management | Placeholder row in Models tab |
| Kokoro TTS | Placeholder binary |
| macOS Metal backend | Fetch script written, untested |

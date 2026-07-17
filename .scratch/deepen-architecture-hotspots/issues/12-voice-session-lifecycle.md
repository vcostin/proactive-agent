# 12 — VoiceSession capture lifecycle

**What to build:** Starting and stopping the mic goes through one VoiceSession module. Commands no longer own capture-thread spawn, channel wiring, or stop-flag swap. Soft-fail when Host STT is unavailable keeps mic/waveform up with transcription off and a clear Setup repair hint. The unused alternate capture handle path is removed so only one capture lifecycle remains.

**Blocked by:** None — can start immediately (prefer after ticket 11).

**Status:** resolved

- [x] VoiceSession start/stop (and Drop) hide thread, channel, and stop-flag details behind a small interface
- [x] Soft-fail: mic starts when Host STT engine is absent; transcription off; repair-oriented diagnostics preserved (ADR 0001)
- [x] Unused alternate capture handle path deleted or absorbed — one capture lifecycle only
- [x] Idempotent stop/Drop (double-stop / remount safe)
- [x] Seam tests with injected fake capture adapter — no live mic or WebView required
- [x] UtterancePipeline / HostSttEngine load extraction remain out of scope

## Answer

`VoiceSession` (`src-tauri/src/audio/voice_session.rs`) owns capture thread / mpsc / stop-flag; commands are thin adapters over `SharedVoiceSession`. Soft-fail when STT absent keeps mic up with `TranscriptionMode::Off` + Setup repair hint. Deleted unused `start_capture` / `VoiceHandle`. Seam tests use `FakeCapture` + `RecordingLog`.

## Comments

- Implemented on branch `research/setup-status-vs-verify-catalog` (VoiceSession deepening).

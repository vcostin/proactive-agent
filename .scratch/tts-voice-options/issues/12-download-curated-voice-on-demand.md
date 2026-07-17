# 12 — Download curated voice on demand

**What to build:** User can download a non-installed curated Piper voice (onnx+json, Hugging Face `v1.0.0` pin). Show progress/busy and errors; only mark installed when both files verify. Failed download leaves the prior voice selection intact.

**Blocked by:** 11 — Curated catalog + installed detection

**Status:** ready-for-human

- [x] Download fetches both `.onnx` and `.onnx.json` for a curated id
- [x] In-progress and failure states are visible; failure does not corrupt selected voice id
- [x] After success, catalog reports the voice as installed
- [x] Setup Wizard stays default-voice-only (no full curated wizard steps)

## Comments

Implemented: `download_curated_piper_voice` + streaming `HttpVoiceFileFetcher` in `audio/piper_download.rs`; curated `hf_stem` metadata; Tauri `download_curated_voice` emits `download_progress` (with `voice_id`) and does not mutate `tts_voice_id`. Seam tests cover success, failed json (no orphan onnx), unknown id, and wizard catalog default-only. Settings picker busy/error chrome is issue 13.

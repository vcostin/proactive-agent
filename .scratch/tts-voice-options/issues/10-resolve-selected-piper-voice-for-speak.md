# 10 — Resolve selected Piper voice for speak

**What to build:** Voice output speaks using a configurable Piper voice id from config (default `en_US-lessac-medium`). Resolution requires both `.onnx` and `.onnx.json`; a missing/invalid selection falls back to the default. Existing chat/proactive speak keeps working with the default voice.

**Blocked by:** None — can start immediately.

**Status:** ready-for-human

- [x] Config stores a stable Piper voice id (default lessac-medium)
- [x] Speak resolves onnx+json for the selected id; missing selection falls back to default
- [x] Default voice path also requires the sibling `.onnx.json` (not onnx-only)
- [x] Seam tests cover resolve + fallback; Voice mute / Core agent without Piper unchanged

## Comments

Implemented: `resolve_piper_voice` + `AppConfig.tts_voice_id`; `speak_text` resolves via config. Linux setup now requires default `.onnx.json` (not soft-fail). Hands-on: unmute Voice output and confirm chat still speaks with lessac when both files are present.

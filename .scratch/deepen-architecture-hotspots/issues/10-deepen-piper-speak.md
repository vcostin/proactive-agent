# 10 — Deepen Piper speak (absorbs preview)

**What to build:** Chat replies, proactive speak, and Settings voice Preview all go through one deep Piper speak module. Callers no longer assemble path layout, cancel tokens, or voice resolve themselves. Preview plays a fixed sample without changing the saved voice; if the curated pair is missing it installs via today’s download path first. A new speak supersedes in-flight playback. Mute and download-progress UI stay as they are.

**Blocked by:** None — can start immediately.

**Status:** resolved

- [x] Speak for a voice id owns path resolution, PlaybackGate, Piper spawn, and playback; Tauri speak adapter does not pass gate tokens or vestigial client construction
- [x] Preview uses the same module with a single fixed-sample source of truth and does not persist `tts_voice_id`
- [x] Missing curated onnx+json on preview triggers ensure-installed via existing curated download, then speaks; selection unchanged
- [x] Starting speak or preview supersedes in-flight PCM playback (no overlapping samples)
- [x] Seam tests cover speak, preview (no config mutate), and supersede at the speak interface — not only leaf WAV helpers
- [x] Existing mute, voice picker, and download-progress behaviour still work

## Answer

`PiperSpeak` owns models→tts path, PlaybackGate, resolve, preview fixed sample, and ensure-installed. Production `PiperPlayEngine` wraps Piper spawn + cpal. Tauri `speak_text` / `preview_voice` are thin adapters over `SharedPiperSpeak`. Seam tests in `audio/piper_speak.rs`.

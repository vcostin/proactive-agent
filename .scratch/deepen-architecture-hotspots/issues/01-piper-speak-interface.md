# Piper speak module interface (absorbs preview)

Type: grilling
Status: resolved
Blocked by:

## Question

What is the external **interface** of the deepened Piper speak module — method(s), parameters, error modes — such that callers (Tauri command adapters) no longer own `tts/` path join, `PlaybackGate::begin`, resolve, or vestigial `TtsClient::new(0)`?

Must cover: speak configured voice; speak/preview a target voice id **without** mutating `tts_voice_id`; cancel/supersede in-flight playback; fixed preview sample as a single source of truth. What stays outside the seam (mute, config persistence, progress UI, download transport)?

## Answer

Locked in [spec.md](../spec.md) Implementation Decisions → Piper speak: deep module owns path/gate/resolve/piper/playback; preview = fixed sample without mutating config; ensure-installed reuses today’s download path; prefer log/event adapter over AppHandle on the core interface; mute/persist/progress stay outside.

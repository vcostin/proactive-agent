# 13 — Settings Voice picker + persist

**What to build:** Near the Voice output mute control, user can pick an installed curated voice; the choice persists across restart and applies to the next chat/proactive speak. Mute remains independent of which voice is selected.

**Blocked by:** 11 — Curated catalog + installed detection; 12 — Download curated voice on demand

**Status:** ready-for-human

- [x] Settings/Voice UI lists curated voices with installed/available affordances
- [x] Selecting an installed voice persists the voice id and affects the next speak
- [x] After quit/reopen, the same voice is still selected and used
- [x] Voice mute still silences speech regardless of selection

## Comments

Implemented: `VoicePicker` beside mute in chat header; Tauri `list_curated_voices` / `get_tts_voice` / `set_tts_voice` (select only when curated+installed via `ensure_selectable_piper_voice`); download busy/error chrome reuses `download_curated_voice` + `download_progress`. Mute stays frontend-only and untouched by selection. Seam tests cover selectable vs not-installed / unknown id. Preview is issue 14.

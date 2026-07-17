# 14 — Voice preview (fixed sample)

**What to build:** User can preview a curated voice with a fixed sample sentence via the same Piper speak path. Not-yet-installed voices download first (ticket 12). Starting preview replaces in-flight playback so samples do not overlap.

**Blocked by:** 10 — Resolve selected Piper voice for speak; 11 — Curated catalog + installed detection; 12 — Download curated voice on demand

**Status:** ready-for-agent

- [ ] Preview speaks the fixed sample in the target voice
- [ ] Preview of a missing curated voice downloads then speaks (or prompts download then speaks)
- [ ] New preview/speak replaces in-flight Piper playback
- [ ] Preview does not require changing the persisted selected voice unless the user also Selects

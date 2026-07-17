# 11 — Curated catalog + installed detection

**What to build:** The curated P0 Piper list (lessac, joe, kristin, bryce, sam, cori — medium) is queryable with installed vs available state (both onnx+json present under models/tts).

**Blocked by:** 10 — Resolve selected Piper voice for speak

**Status:** ready-for-human

- [x] Curated catalog exposes the locked P0 voice ids with human-readable labels/locale
- [x] Installed means both `.onnx` and `.onnx.json` exist for that id
- [x] Default lessac reports installed when its pair is present
- [x] Catalog/installed detection covered at the voice-catalog seam

## Comments

Implemented: `list_curated_piper_voices` in `audio/piper_catalog.rs` (P0 metadata + installed via shared `piper_voice_pair_present`). Seam tests cover curated set, empty dir, lessac pair, and onnx-only.
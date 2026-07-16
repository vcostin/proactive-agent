# 01 — Core setup gate + status seam tests

**What to build:** When Core agent requirements are met (chat model + inference readiness as required for chat), the main UI is reachable without Piper/TTS as a hard gate. Setup status exposes STT readiness so Host STT can be checked from the same seam the wizard uses. Fixture-based tests lock status/gate behaviour at the Rust setup command surface without UI e2e.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] Main UI is allowed when Core agent requirements are met even if Piper/TTS artifacts are absent
- [x] Setup status reports STT readiness distinctly from Core agent readiness
- [x] Fixture tests at the setup command seam cover present/absent chat model, llama, piper, and STT-related artifacts
- [x] Existing Host Linux Core agent chat path still works when artifacts are present
- [x] No Target STT (`ort`) or new TTS product behaviour is introduced

# 06 — Host STT visible and repairable via wizard

**What to build:** On the Host OS (Linux), Host STT (mic → text) app-managed pieces are visible in setup status and can be repaired through the Setup Wizard without regressing the already-working mic path. Guest OS missing STT degrades with a clear repair state — Core agent does not crash. Target STT (`ort`) is not implemented.

**Blocked by:** 02 — Platform-module artifact catalog (expand); 04 — Setup Wizard consumes Platform modules

**Status:** done

- [x] Setup status surfaces Host STT-related readiness clearly on Linux
- [x] Setup repair can detect and restore missing Host STT app-managed artifacts covered by the catalog
- [x] Existing Linux mic → text path still works when artifacts are present
- [x] Missing STT on Guest paths does not crash shared core; user is directed to repair/not-ready messaging
- [x] No Target STT (`ort`) migration work is included

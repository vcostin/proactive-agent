# 04 — Setup Wizard consumes Platform modules

**What to build:** The Setup Wizard (first-run and repair downloads) installs and verifies app-managed artifacts using the Platform-module catalog. Re-runs are idempotent when artifacts already exist. Shipped-app error/recovery copy points users at the Setup Wizard / Setup repair, not Developer setup.

**Blocked by:** 02 — Platform-module artifact catalog (expand)

**Status:** done

- [x] Wizard download/verify paths resolve artifacts via Platform-module definitions for the running OS
- [x] Re-running download/verify when artifacts are present is safe (idempotent)
- [x] Partial failure does not needlessly destroy already-good artifacts when avoidable
- [x] User-facing recovery hints prefer Setup Wizard / Setup repair over Developer setup
- [x] Tests at the setup command seam cover catalog-driven verify/download behaviour with fixtures/fake HTTP where practical
- [x] No new product features (TTS, Target STT) are added through the wizard

# 12 — Download curated voice on demand

**What to build:** User can download a non-installed curated Piper voice (onnx+json, Hugging Face `v1.0.0` pin). Show progress/busy and errors; only mark installed when both files verify. Failed download leaves the prior voice selection intact.

**Blocked by:** 11 — Curated catalog + installed detection

**Status:** ready-for-agent

- [ ] Download fetches both `.onnx` and `.onnx.json` for a curated id
- [ ] In-progress and failure states are visible; failure does not corrupt selected voice id
- [ ] After success, catalog reports the voice as installed
- [ ] Setup Wizard stays default-voice-only (no full curated wizard steps)

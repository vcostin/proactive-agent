# TTS on proactive nudges

Type: grilling
Status: open
Blocked by:

## Question

When a deferred message fires, under what conditions should it be spoken via the existing Piper / `speak_text` path (same as chat replies gated by the voice-output toggle) — always if TTS is on, only when focused, never if muted mid-session, etc. — and where does that responsibility live (frontend `addProactive` vs Rust)?

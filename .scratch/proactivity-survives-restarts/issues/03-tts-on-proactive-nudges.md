# TTS on proactive nudges

Type: grilling
Status: resolved
Blocked by:

## Question

When a deferred message fires, under what conditions should it be spoken via the existing Piper / `speak_text` path (same as chat replies gated by the voice-output toggle) — always if TTS is on, only when focused, never if muted mid-session, etc. — and where does that responsibility live (frontend `addProactive` vs Rust)?

## Answer

Speak whenever Voice output is on at delivery time (live event or overdue flush) — same session toggle as chat replies; no focus gate. Responsibility lives in the frontend delivery path: `deliverProactive` / `useChat.addProactive` calls `speak_text`. Rust only transports due items; it does not own TTS.

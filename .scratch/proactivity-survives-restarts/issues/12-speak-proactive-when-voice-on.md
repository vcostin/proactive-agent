# 12 — Speak proactive when Voice output is on

**What to build:** When Voice output is on, a delivered proactive nudge is spoken on the same TTS path as chat replies; when off, it stays text-only.

**Blocked by:** 11 — Overdue nudge appears after reopen

**Status:** done

- [x] With voice-output on, a fired/flushed proactive message is spoken
- [x] With voice-output off, the proactive message appears as text only
- [x] Core agent chat still works if TTS/Piper is unavailable (text delivery unaffected)

## Notes

Delivery seam: `deliverProactive` (`src/chat/deliverProactive.ts`) — always appends text; calls `speak` only when voice-output is on and content is non-empty; speak rejection is swallowed so text stays. Wired from `useChat.addProactive` via existing `speak_text`. Deno tests: `deno task test:chat`.

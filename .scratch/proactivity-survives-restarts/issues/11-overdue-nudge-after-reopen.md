# 11 — Overdue nudge appears after reopen

**What to build:** A deferred message whose fire time passed while the app was closed appears in chat soon after reopen (UI-ready flush), so restart does not drop overdue nudges.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** done

- [x] Close with a past-due pending defer → reopen → proactive message appears in chat
- [x] Overdue delivery does not depend on a racey startup event before listeners attach
- [x] Several overdue items all deliver (none silently dropped)
- [x] Background loop still fires future-due items after the initial flush

## Notes

Implemented on `feat/proactivity-survives-restarts`: `flush_due_deferred` + `useProactiveEvents` mount flush (invoke return path); scheduler loop skips first tick; seam test covers multi-overdue drain leaving future pending. Hands-on reopen check still recommended before merge to master.

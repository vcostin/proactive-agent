# 11 — Overdue nudge appears after reopen

**What to build:** A deferred message whose fire time passed while the app was closed appears in chat soon after reopen (UI-ready flush), so restart does not drop overdue nudges.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** ready-for-agent

- [ ] Close with a past-due pending defer → reopen → proactive message appears in chat
- [ ] Overdue delivery does not depend on a racey startup event before listeners attach
- [ ] Several overdue items all deliver (none silently dropped)
- [ ] Background loop still fires future-due items after the initial flush

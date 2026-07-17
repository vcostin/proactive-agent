# 14 — Cancel from Debug → Scheduler

**What to build:** From Debug → Scheduler, the user can cancel a pending deferred message by id; it disappears from the panel and stays gone after restart.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** ready-for-agent

- [ ] Cancel removes the pending item from Debug → Scheduler
- [ ] After cancel, quitting and reopening does not bring the item back
- [ ] Fire-now remains available for testing immediate delivery

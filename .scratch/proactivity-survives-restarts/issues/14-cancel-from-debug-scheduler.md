# 14 — Cancel from Debug → Scheduler

**What to build:** From Debug → Scheduler, the user can cancel a pending deferred message by id; it disappears from the panel and stays gone after restart.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** done

- [x] Cancel removes the pending item from Debug → Scheduler
- [x] After cancel, quitting and reopening does not bring the item back
- [x] Fire-now remains available for testing immediate delivery

## Notes

Already shipped on `feat/proactivity-survives-restarts` (persist commit): Debug → Scheduler `cancel` invokes `cancel_deferred`; `ProactivityScheduler::cancel` removes by id and persists; `scheduler_updated` refreshes the panel; seam test `cancel_removes_by_id_and_persists` locks durable removal across reload. Fire-now is unchanged.

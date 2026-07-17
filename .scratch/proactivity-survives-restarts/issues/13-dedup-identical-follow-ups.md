# 13 — Dedup identical follow-ups

**What to build:** Scheduling a second deferred follow-up with the same message text and trigger replaces the existing pending item instead of stacking duplicates.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** done

- [x] Two adds with identical message+trigger leave a single pending row
- [x] Different trigger with the same message text may still coexist
- [x] Dedup behaviour is covered at the scheduler seam

## Notes

Already shipped on `feat/proactivity-survives-restarts` (persist commit): `ProactivityScheduler::add` retains by replacing same `message`+`trigger`; chat/`test_defer` paths both go through `add`. Seam test `add_replaces_pending_with_same_message_and_trigger` locks identical-key replace and different-trigger coexistence.

# 13 — Dedup identical follow-ups

**What to build:** Scheduling a second deferred follow-up with the same message text and trigger replaces the existing pending item instead of stacking duplicates.

**Blocked by:** 10 — Persist deferred queue across restart

**Status:** ready-for-agent

- [ ] Two adds with identical message+trigger leave a single pending row
- [ ] Different trigger with the same message text may still coexist
- [ ] Dedup behaviour is covered at the scheduler seam

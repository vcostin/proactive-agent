# 10 — Persist deferred queue across restart

**What to build:** A deferred follow-up scheduled for the future remains pending after the user quits and reopens the app, visible in Debug → Scheduler. The queue lives as JSON beside config and reloads on launch; mutations persist immediately.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] After scheduling a future deferred message, quitting and reopening shows the same pending item in Debug → Scheduler
- [x] Missing queue file on first launch behaves as empty pending (no crash)
- [x] Scheduler-seam tests cover JSON round-trip of pending deferred messages

## Notes

Implemented on `feat/proactivity-survives-restarts`: `ProactivityScheduler` persists to JSON beside config, loads on app start, seam tests include round-trip + missing-file → empty pending. Hands-on reopen check still required before merge to master.

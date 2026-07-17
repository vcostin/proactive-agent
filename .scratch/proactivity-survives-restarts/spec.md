Status: ready-for-agent

# Spec: Proactivity that survives restarts

Source map: `.scratch/proactivity-survives-restarts/map.md` (ROADMAP §1 P0).  
Branch note: `feat/proactivity-survives-restarts` already implements this behaviour; this spec is the handoff collapse so tickets and review have a single source of truth.

## Problem Statement

The Core agent can schedule follow-ups with `<defer>`, but the deferred queue lived only in memory. Closing the Proactive Agent dropped pending nudges, so “proactive” did not survive leaving and coming back. Debug tools could fire a pending item but not cancel it, identical follow-ups could stack, and spoken Voice output did not cover proactive bubbles the way it covers chat replies.

## Solution

Persist the deferred queue next to config, reload it on launch, and deliver overdue items when the UI is ready (so a restart still produces the nudge). When Voice output is on, speak proactive messages on the same TTS path as chat replies. Deduplicate identical follow-ups, allow cancel from Debug → Scheduler, and keep the existing persona guidance that nudges the model to emit `<defer>` for useful follow-ups.

**Done when:** close the app with a pending defer → reopen later → message appears (and is spoken if TTS is on).

## User Stories

1. As a user, I want a deferred follow-up to survive quitting the app, so that the agent can still nudge me later.
2. As a user, I want overdue deferred messages to appear soon after I reopen the app, so that I do not lose a nudge that became due while I was away.
3. As a user with Voice output on, I want overdue and on-time proactive nudges spoken aloud, so that I notice them without reading the chat.
4. As a user with Voice output off, I want proactive nudges to appear as text only, so that the app stays quiet.
5. As a user, I want the proactive bubble to show in chat history like other messages, so that I can scroll back to what the agent said.
6. As a user, I want the model’s `<defer>` tag stripped from the visible assistant reply, so that I only see the follow-up when it fires.
7. As a user, I want useful follow-ups to be scheduled when a topic is unresolved, so that proactivity feels intentional rather than random.
8. As a user, I do not want dozens of identical pending follow-ups for the same topic, so that the queue stays sane.
9. As a developer debugging proactivity, I want to see pending deferred messages in Debug → Scheduler, so that I can inspect fire times and triggers.
10. As a developer, I want to fire a pending deferred message immediately from Debug → Scheduler, so that I can test delivery without waiting.
11. As a developer, I want to cancel a pending deferred message from Debug → Scheduler, so that I can clear bad or duplicate schedules without restarting.
12. As a developer, I want a test-defer shortcut in Debug (dev builds), so that I can exercise the pipeline without waiting for the model to emit `<defer>`.
13. As a developer, I want the Scheduler panel to refresh promptly after add/cancel/fire/flush, so that Debug state matches the queue.
14. As a maintainer, I want the deferred queue stored as JSON beside config (not in LanceDB), so that a small schedule stays independent of memory search.
15. As a maintainer, I want mutations to persist the queue immediately, so that a crash after schedule still keeps the item.
16. As a maintainer, I want load-on-launch of a missing queue file to mean “empty pending,” so that first run is clean.
17. As a maintainer, I want overdue delivery on UI mount via an invoke return path (not only a fire-and-forget event), so that restart delivery does not race listeners.
18. As a maintainer, I want the background scheduler loop to keep firing future-due items after launch, so that in-session delays still work.
19. As a maintainer, I want the first scheduler tick skipped after startup, so that overdue ownership stays with the UI flush path.
20. As a maintainer, I want dedup keyed by identical message text and trigger (replace pending), so that a refreshed follow-up updates timing instead of stacking clones.
21. As a maintainer, I want cancel-by-id to remove and persist, so that Debug cancel is durable across restart.
22. As a maintainer, I want default persona prompt guidance for `<defer>` left as the P0 nudge (no new deferral subsystem), so that prompt work stays a light confirm/tune.
23. As a maintainer, I want lenient `<defer>` parse with discard on failure preserved, so that malformed tags do not break the reply.
24. As a user, I want Core agent chat to keep working if TTS/Piper is unavailable, so that proactive text delivery does not depend on spoken Voice.
25. As a QA user, I want to schedule a defer, quit, reopen after due time, and see the message, so that the done-when path is verifiable by hand.
26. As a QA user, I want to repeat that path with Voice output on and hear the nudge, so that TTS-on-proactive is verifiable.
27. As a QA user, I want to cancel a pending item in Debug and confirm it is gone after restart, so that cancel persistence is verifiable.
28. As a QA user, I want two identical message+trigger defers to leave a single pending row, so that dedup is verifiable.
29. As an agent implementing or reviewing this, I want one primary Rust scheduler seam for queue behaviour, so that tests lock persist/dedup/cancel/overdue without UI e2e.
30. As an agent, I want delivery+TTS asserted at the chat/proactive delivery path, so that “spoken if TTS on” is specified without redesigning Piper.
31. As a user, I want several overdue items that became due while closed to all appear after reopen, so that a backlog is not silently dropped.
32. As a user mid-conversation, I want a due deferred message to still appear when its time comes, so that in-session proactivity continues after restart recovery.
33. As a maintainer, I want `last_fired` to remain in-memory status for Debug, not a required on-disk field, so that the persisted schema stays minimal.
34. As a maintainer, I want proactive delivery to update local chat history the same way as today’s proactive bubbles, so that restart-delivered nudges are not lost on refresh of the UI session.
35. As a maintainer, I do not want this P0 to require writing episodic LanceDB memory on fire beyond existing chat-history behaviour, so that memory distillation stays orthogonal.
36. As an end user, I do not want a separate user-facing schedule manager in this P0, so that scheduling stays model-driven with Debug-only controls.
37. As an end user, I do not need to edit pending defer text or fire time in this P0, so that cancel and fire-now are enough operator tools.
38. As a maintainer, I want Host OS behaviour verified first for this path, so that Guest OS quirks do not block the Core agent proactivity done-when.
39. As a maintainer, I want failure to persist the queue logged without crashing the Core agent, so that a disk error degrades to best-effort rather than bricking chat.
40. As a reviewer, I want this spec’s out-of-scope list to match the wayfinder map, so that `/implement` and `/code-review` do not expand into sync, TTS redesign, or stricter parsing.

## Implementation Decisions

- **Primary seam:** `ProactivityScheduler` — pending deferred messages, persist/load, drain due, dedup on add, cancel by id, fire-now. Prefer extending this module over scattering queue rules in the UI.
- **Delivery seam:** UI mount flushes due items via invoke (`flush_due_deferred`) and applies them through the existing proactive chat path; live due items after mount continue via `proactive_message` events. TTS uses the same voice-output toggle and `speak_text` path as assistant replies.
- **Store:** JSON file beside `config.json` (name: deferred queue file next to config). Not LanceDB.
- **Minimal on-disk schema:** envelope with a `pending` array of deferred messages (`id`, `message`, `trigger`, `fire_at`). Do not require persisting `last_fired`.
- **Flush timing:** persist on every mutating operation (add, cancel, fire-now, drain-due).
- **Overdue policy:** on UI ready, drain all past-due items and deliver them (fire immediately), not re-schedule or drop.
- **Startup race:** do not rely on emitting overdue events before listeners exist; overdue flush is owned by the UI invoke path; background loop skips its immediate first tick.
- **Dedup:** same `message` + `trigger` replaces the existing pending item (new timing/id wins as implemented by remove-then-add).
- **Cancel API:** cancel by id from Debug → Scheduler; persist removal; refresh Debug status (polling and/or scheduler-updated signal).
- **TTS:** when voice-output toggle is on, speak proactive content on delivery; when off, text only. No TTS engine/architecture change.
- **Persona nudge:** keep existing default persona prompt guidance for `<defer>`; no new deferral system in P0.
- **Parse policy:** keep lenient `<defer>` parsing; discard malformed JSON tags without failing the visible reply.
- **Memory:** proactive fire updates chat UI / local chat history; P0 does not add a new LanceDB episodic write requirement on fire.
- **Multiple overdue:** drain returns all due items; deliver each (order = drain order).
- **Mid-turn / streaming:** no special queue pause in P0 — due items may appear while the user is active; do not block delivery on idle.
- **Vocabulary:** Core agent, deferred follow-ups / deferred messages, Voice / TTS, Debug → Scheduler — align with `CONTEXT.md` (TTS remains WIP architecturally; this spec only wires existing speak path to proactive delivery).

## Testing Decisions

- **Good tests** assert external behaviour at the scheduler seam: given known deferred messages and a temp queue path, round-trip persist, overdue drain clears pending and returns due, dedup replaces, cancel removes and persists. Do not assert private helpers or React internals as the primary suite.
- **Modules under test:** `ProactivityScheduler` (persist, load, drain, dedup, cancel). Delivery/TTS may be covered lightly at the hook/command boundary if a frontend test harness exists; otherwise hands-on QA covers the done-when path.
- **Prior art:** Rust `#[cfg(test)]` modules elsewhere in the crate (e.g. audio/TTS cleaners); scheduler unit tests in the same style with temp files.
- **Regression bar:** close with pending defer → reopen after due → message appears; with voice-output on, it is spoken; cancel survives restart; identical message+trigger does not stack.

## Out of Scope

- User-facing schedule UI beyond Debug → Scheduler
- Multi-device / sync of the deferred queue
- Editing pending defer text or fire time (cancel + fire-now only)
- Changing TTS engine or architecture
- Stricter `<defer>` parsing (keep lenient discard-on-failure)
- New LanceDB-backed schedule store
- Re-scheduling overdue instead of firing (unless a future effort redraws the destination)
- Setup Wizard / Platform-module work
- GPU offload, Host STT changes, or other ROADMAP phases

## Further Notes

- Wayfinder destination for this effort was this thin handoff spec; map Decision tickets may still be open — treat this document as the collapsed authority for build/review.
- Hands-on verification on the Host OS remains required before merging to `master` per project contributing rules.
- If domain terms for “deferred message” vs “proactive nudge” should enter `CONTEXT.md`, do that under `/domain-modeling` in a follow-up — not required to ship this P0.

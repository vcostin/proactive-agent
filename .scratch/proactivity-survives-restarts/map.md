# Proactivity that survives restarts

Labels: `wayfinder:map`

## Destination

A thin, implementable **spec** for ROADMAP §1 such that handoff is unambiguous: close the app with a pending defer → reopen later → message appears (and is spoken if TTS is on). Decisions live on tickets; this map ends when nothing material is left to decide before someone codes.

## Notes

- Domain: Core agent proactivity / deferred follow-ups. Consult root `CONTEXT.md`, `ROADMAP.md` §1, `ARCHITECTURE.md` (scheduler / `<defer>`), and `src-tauri/src/orchestrator/scheduler.rs`.
- Skills every session should consult: `/grilling`, `/domain-modeling` (and `/prototype` only if a ticket asks for an artifact).
- Plan, don't build the feature in this map — produce decisions, then the handoff spec. Prompt-nudge work is **confirm/tune the existing default `persona_prompt`**, not a new deferral system.
- Note: `feat/proactivity-survives-restarts` already shipped a working persist path; this map is catching Decisions so far up to that reality (ratify, don't re-litigate unless you want to change course).
- Current code fact (branch): queue persists to `deferred_queue.json` beside config; overdue flush via `flush_due_deferred` on UI mount; cancel + TTS-on-nudge exist; default persona already mentions `<defer>`.

## Decisions so far

- TTS on proactive: speak iff Voice output is on at delivery; responsibility is frontend `deliverProactive` / `addProactive` (same `speak_text` as replies) — see issues/03, issues/12.

## Not yet specified

- Whether firing a deferred message also writes episodic / chat-history side effects beyond the UI bubble (depends on how delivery is described in the TTS / delivery tickets).
- Exact on-disk schema fields and flush timing (every mutation vs batched) — graduates after the persist decision.
- Behaviour when several overdue items are due at once while the user is mid-turn / streaming.

## Out of scope

- User-facing schedule UI beyond Debug → Scheduler
- Multi-device / sync of the deferred queue
- Editing pending defer text or fire time (cancel + fire-now only)
- Changing TTS engine or architecture
- Stricter `<defer>` parsing (keep lenient discard-on-failure)

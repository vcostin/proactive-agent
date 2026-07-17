# 13 — Settings Voice picker + persist

**What to build:** Near the Voice output mute control, user can pick an installed curated voice; the choice persists across restart and applies to the next chat/proactive speak. Mute remains independent of which voice is selected.

**Blocked by:** 11 — Curated catalog + installed detection; 12 — Download curated voice on demand

**Status:** ready-for-agent

- [ ] Settings/Voice UI lists curated voices with installed/available affordances
- [ ] Selecting an installed voice persists the voice id and affects the next speak
- [ ] After quit/reopen, the same voice is still selected and used
- [ ] Voice mute still silences speech regardless of selection

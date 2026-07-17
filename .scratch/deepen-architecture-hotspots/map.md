# Deepen architecture hotspots

Labels: `wayfinder:map`

## Destination

One combined **architecture + acceptance** spec covering three deepenings — **Piper speak** (absorbs preview), **Platform readiness** (catalog → SetupStatus / wizard gates), **VoiceSession** (capture lifecycle) — handoff-ready for implementation agents. Done when that `spec.md` is locked; no build-ticket slicing in this map.

## Notes

- Source review: `/tmp/architecture-review-20260717-213712.html`
- Domain: Voice TTS (WIP), Host STT path, Platform module / Setup Wizard — consult `CONTEXT.md`, ADR 0001 (in-process ort only; no sidecar reopen), `/codebase-design` vocabulary (module, interface, depth, seam, adapter, leverage, locality)
- Skills: `/grilling`, `/domain-modeling`; `/research` for readiness SSOT facts; `/prototype` only if a ticket asks
- Plan, don't build — decisions then the handoff spec
- Prescribed section order in the spec: Piper speak → Platform readiness → VoiceSession
- Spec fidelity: problem, deepened interface, absorb vs stay-outside, test seams, short acceptance checklist — not full product PRDs

## Decisions so far

- SetupStatus and Platform `verify_catalog`/`required_for_*` are parallel today (wizard uses setup only; catalog verify unused by FE) — see [SetupStatus vs Platform verify_catalog — current duplication](issues/02-setup-status-vs-verify-catalog.md).
- [Piper speak module interface (absorbs preview)](issues/01-piper-speak-interface.md) — Locked in [spec.md](spec.md): speak owns path/gate/resolve; preview = fixed sample + no config mutate; prefer log adapter over AppHandle on core interface.
- [Platform readiness derivation interface](issues/03-platform-readiness-derivation.md) — Locked in [spec.md](spec.md): SetupStatus derives from verify + `required_for_*`; wizard is presentation adapter; chat model still in Core ready.
- [VoiceSession capture lifecycle interface](issues/04-voice-session-interface.md) — Locked in [spec.md](spec.md): start/stop + soft-fail; delete unused capture handle; UtterancePipeline out.
- [Write the combined deepening handoff spec](issues/05-write-combined-handoff-spec.md) — Published [spec.md](spec.md) (`ready-for-agent`).

## Not yet specified

- (none — destination spec published)

## Out of scope

- Curated voice inventory deepening (catalog + download + pair presence as one module)
- Deferred delivery deepening (FE+BE proactive orchestration)
- HostSttEngine (`try_load` out of `lib.rs` / one ORT resolver) — follow-on after readiness SSOT
- UtterancePipeline (Tauri-free STT event sink / dual VAD collapse)
- CatalogFetcher / `download_all` / script destination-layout SSOT (fetch orchestration after “ready?”)
- Implementing any deepening
- Numbered build tickets inside this map
- Reopening ADR 0001 expand–contract or Parakeet HTTP sidecar

Status: resolved

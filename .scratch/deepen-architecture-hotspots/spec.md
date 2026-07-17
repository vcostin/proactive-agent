Status: resolved

# Spec: Deepen architecture hotspots

Source map: `.scratch/deepen-architecture-hotspots/map.md`  
Source review: architecture deepening review (Piper speak, Platform readiness, VoiceSession)

## Problem Statement

Recent Voice TTS, Host STT, and Setup work left three shallow hotspots. Callers must reassemble “speak a Piper voice,” “is the Host ready?,” and “start the mic” from path layout, gate tokens, catalog flags, and thread wiring. Setup Wizard readiness still runs beside Platform `verify_catalog` instead of through it, so “ready?” has two answers. Developers and agents cannot test the real bug sites without Tauri, cpal, or brittle UI, and small product changes keep leaking across those seams.

## Solution

Deepen three modules behind small interfaces, in order: **Piper speak** (absorbs preview), **Platform readiness** (catalog → setup status / wizard gates), **VoiceSession** (capture lifecycle). Tauri commands and the Setup Wizard become thin adapters. Acceptance is architectural and behavioural at those seams — not a new end-user product feature set.

**Done when:** speak/preview go through one speak interface with cancel/supersede; SetupStatus (or successor) derives from Platform verify + `required_for_*`; mic start/stop goes through VoiceSession with soft-fail when Host STT is unavailable; tests lock those three seams.

## User Stories

1. As a user with Voice output on, I want chat replies spoken through Piper without caring about model paths or cancel tokens, so that spoken output just works.
2. As a user, I want starting a new speak or preview to stop the previous one, so that samples and replies do not overlap.
3. As a user, I want Preview on a curated voice to play a fixed sample without changing my saved voice selection, so that I can try a voice safely.
4. As a user, I want Preview on a not-yet-installed curated voice to download what it needs then play, still without persisting selection, so that Preview stays one action.
5. As a user with Voice output off (muted), I want speak/preview to stay silent, so that mute remains a frontend gate.
6. As a user, I want the default and curated Piper voices to keep resolving with onnx+json pairs, so that Piper does not fail on missing config.
7. As a user opening Setup repair, I want “Core agent ready” and “Host STT ready” to match the Platform catalog’s required artifacts, so that the wizard does not disagree with itself.
8. As a user, I want the Setup Wizard to remain a setup/repair surface only, so that deepening readiness does not ship new product features through the wizard.
9. As a user on the Host OS, I want Core agent usable when chat model + llama are ready even if Voice TTS artifacts are missing, so that Piper stays out of the Host completion bar.
10. As a user on the Host OS, I want Host STT readiness to mean encoder + decoder + vocab + ONNX Runtime lib present and verifiable, so that mic→text expectations match ADR 0001.
11. As a user, I want missing or broken Host STT artifacts to point me at Setup repair, so that soft-fail stays recoverable.
12. As a user, I want to start the mic when Host STT is soft-failed and still see waveform/energy, so that the Core agent path is not bricked by Voice.
13. As a user, I want stopping the mic to tear down capture cleanly, so that restarting Voice input does not leave zombie capture threads.
14. As a user, I want closing the app to stop Voice capture, so that sidecars and devices are not left busy.
15. As a developer, I want Tauri speak/preview commands to be thin adapters over one speak module, so that path join and PlaybackGate sequencing leave the command surface.
16. As a developer, I want a single preview sample string owned by the speak/preview path, so that Debug and Settings cannot drift.
17. As a developer, I want ensure-installed for preview to reuse today’s curated download path without inventing a Voice inventory module, so that inventory deepening stays out of this effort.
18. As a developer, I want `SetupStatus` (or successor) derived from `verify_catalog` and `required_for_core` / `required_for_stt`, so that catalog is the single readiness answer.
19. As a developer, I want the Setup Wizard to present derived status rather than re-deriving gates from ad-hoc booleans, so that wizard steps cannot drift from the Platform module.
20. As a developer, I want chat-model presence to remain part of Core readiness even though it is not a catalog row today, so that `ready` still means the Core agent can chat.
21. As a developer, I want catalog-only rows that are not Core/STT required (e.g. vulkan libs, default tts-voice) to stay visible for repair/download without falsely gating Core ready, so that required flags stay meaningful.
22. As a developer, I want VoiceSession start/stop to hide thread spawn, channel sizes, and stop-flag swap, so that commands do not own cpal lifecycle.
23. As a developer, I want the unused alternate capture handle path deleted or absorbed, so that there is one Voice capture interface.
24. As a developer, I want VoiceSession to accept an optional Host STT engine and soft-fail transcription when absent, so that ADR 0001 soft-fail policy stays local to Voice lifecycle.
25. As a developer, I want injected sinks for transcripts/energy/debug rather than baking Tauri into the VoiceSession interface forever, so that tests do not need a WebView.
26. As a maintainer, I want these deepenings ordered Piper speak → Platform readiness → VoiceSession, so that hot TTS friction lands first and VoiceSession can assume a single readiness story.
27. As a maintainer, I want tests at the three confirmed seams only (speak interface, derived readiness, VoiceSession with fake capture), so that leaf helper tests are not the sole regression net.
28. As a maintainer, I want Guest OS Platform modules to keep compiling with the same derivation pattern, so that Host-first work does not break Guest modularity.
29. As a maintainer, I do not want this effort to reintroduce the Parakeet HTTP sidecar, so that ADR 0001 hard cutover stays intact.
30. As a maintainer, I do not want HostSttEngine load extraction, UtterancePipeline, CatalogFetcher, curated Voice inventory, or deferred-delivery deepening in this spec, so that scope stays the three Strong hotspots.
31. As an agent implementing Piper speak, I want the interface to accept text + voice id (and preview as speak-fixed-sample), so that callers never pass tts_dir, gate tokens, or vestigial client ports.
32. As an agent implementing Platform readiness, I want one function from layout roots + catalog → status DTO consumed by get_setup_status and the wizard, so that parallel probe_layout formulas can die.
33. As an agent implementing VoiceSession, I want Drop/stop to be idempotent, so that remounts and double-stop do not panic.
34. As a QA developer, I want to supersede an in-flight preview with speak and hear only the latest, so that the speak seam’s cancel behaviour is checkable.
35. As a QA developer, I want to remove one required_for_stt artifact and see Host STT not-ready in SetupStatus without a separate verify UI, so that derivation is checkable.
36. As a QA developer, I want to start Voice input with STT unloaded and confirm mic runs with transcription off and a clear repair hint, so that soft-fail stays Host-debuggable.
37. As a user, I want existing curated voice picker, download progress, and mute UX to keep working after speak deepening, so that Settings Voice does not regress.
38. As a user, I want overdue/proactive and chat TTS behaviour unchanged except that both go through the deepened speak path when speaking, so that proactivity is not redesigned here.
39. As a developer, I want debug logging for TTS/Voice to remain available via an adapter if needed, so that Host diagnostics do not disappear when AppHandle leaves the core speak interface.
40. As a maintainer, I want this spec to be architecture + acceptance handoff only — no numbered build-ticket slicing required inside it — so that implementation agents can cut tickets themselves.

## Implementation Decisions

### Shared

- Produce one combined handoff; implement in order: Piper speak (absorbs preview) → Platform readiness → VoiceSession.
- Use codebase-design vocabulary: deepen modules behind small interfaces; Tauri/React are adapters.
- Respect ADR 0001: Host STT remains in-process ort; no sidecar; soft-fail keeps Core agent up.
- Do not slice numbered build tickets as part of this spec’s deliverable.

### Piper speak (absorbs preview)

- One deep speak module owns: models_dir → tts/ path, resolve_piper_voice + fallback, find_piper + OS library env, Piper stdin→WAV→PCM→resample/upmix→cpal, PlaybackGate begin/supersede, clean_for_speech / empty-text no-op.
- External interface shrinks to speak text for a voice id (configured or explicit); preview is speak of a fixed sample for a target voice id without writing tts_voice_id.
- Callers must not pass gate tokens or vestigial client construction; gate lives inside the module.
- Preview ensure-installed reuses today’s curated download path when the onnx+json pair is missing; does not invent a Voice inventory module.
- Fixed preview sample has a single source of truth shared by Settings Preview and any Debug speak-sample shortcut.
- Stays outside the seam: mute (frontend), persisting tts_voice_id, download progress UI/events, HTTP VoiceFileFetcher details, Setup Wizard default Lessac artifact ownership.
- Prefer a small log/event adapter over requiring AppHandle on the core speak interface long-term; production adapter may emit debug events.

### Platform readiness

- Derive SetupStatus (or successor DTO) from Platform verify_catalog results plus required_for_core / required_for_stt aggregates.
- Core ready continues to require chat model presence (config) in addition to required_for_core catalog artifacts (llama), matching today’s product gate.
- Host STT ready aggregates required_for_stt artifacts (encoder, decoder, vocab, onnxruntime) via catalog verify — not a parallel probe_layout formula.
- Wizard becomes a presentation adapter over derived status; stop re-deriving step gates from independent booleans that disagree with the catalog.
- get_setup_status (or equivalent) is the FE entry; catalog verify is no longer an unused parallel answer for readiness.
- Stays outside this section: CatalogFetcher / download_all / script destination-layout SSOT; HostSttEngine try_load extraction; changing which artifacts are required_for_* beyond aligning status with the catalog as it stands.

### VoiceSession

- VoiceSession module interface: start (optional Host STT engine, energy sink, transcript/event sink) and stop/Drop.
- Implementation absorbs: stop-flag swap, mpsc, dedicated capture thread for !Send cpal, sample-rate handshake, soft-fail mic-on / transcription-off when engine absent.
- Delete or absorb the unused alternate capture handle path so only one capture lifecycle interface remains.
- Stays outside: UtterancePipeline deepening (Tauri-free STT loop / dual VAD collapse), HostSttEngine load-out-of-lib, changing the STT input contract (mono PCM 16 kHz).

## Testing Decisions

- Good tests assert external behaviour at the module interface: outcomes and observable policy, not private helpers or Tauri IPC wiring.
- Confirmed seams (only these three for this effort):
  1. **Speak interface** — speak / preview / supersede (no config mutate on preview; ensure-installed behaviour for missing curated pair as specified).
  2. **Derived readiness** — LayoutRoots + catalog → SetupStatus core/Host STT gates consistent with required_for_* + verify.
  3. **VoiceSession** — start/stop with injected fake capture adapter; soft-fail when STT absent.
- Prefer extending existing Rust unit/seam tests (PlaybackGate, piper resolve, verify_catalog, setup status) by moving assertions up to these seams rather than adding React e2e.
- Prior art: `audio/tts.rs` PlaybackGate tests; `piper_voice` / `piper_catalog` / `piper_download` seam tests; `platform/artifact.rs` verify_catalog tests; `setup/status` probe tests; capture VAD unit tests (lifecycle tests should not require real devices).
- Do not require live microphone, real Piper binary, or WebView for the three seams; use fakes/adapters at the seams.

## Out of Scope

- Curated Voice inventory deepening (catalog + download + pair presence as one public module)
- Deferred delivery deepening (FE+BE proactive orchestration redesign)
- HostSttEngine (`try_load` out of process bootstrap / single ORT resolver) as its own deepening
- UtterancePipeline (Tauri-free event sink, dual VAD collapse)
- CatalogFetcher / download orchestration / Developer setup script destination SSOT
- Implementing numbered build tickets as part of publishing this spec
- New TTS engine, cloud TTS, or full HF voice browser
- Reopening ADR 0001 expand–contract or Parakeet HTTP sidecar
- Making TTS part of Host Setup Wizard completion bar
- Changing STT input contract or Guest OS STT parity requirements

## Further Notes

- Map Notes prescribe architecture + acceptance fidelity (not full product PRD chrome); this document expands user stories for agent handoff while keeping implementation decisions architectural.
- Research baseline for readiness duplication: `.scratch/deepen-architecture-hotspots/research-setup-status-vs-verify-catalog.md` (wizard uses setup status only; verify_catalog unused by FE).
- After implementation, consider glossary updates only if a product-facing term crystallises; module names (e.g. VoiceSession) need not enter `CONTEXT.md` unless they become ubiquitous language.

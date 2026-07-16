Status: ready-for-agent

# Spec: Host STT hard-cutover to Target STT (`ort`)

## Problem Statement

On the Host OS, mic → text still depends on a temporary Parakeet HTTP sidecar (Python / managed launcher / fixed port). That path is fragile to ship, hard to repair cleanly, and blocks treating in-process ONNX Runtime as the real Host STT path. Users and developers already download the Parakeet ONNX model and tokens, but inference still goes through IPC and a separate process. Soft failures are easy to under-diagnose, and expanding a second STT stack beside the sidecar would double Setup repair surface without finishing the cutover.

## Solution

Promote the Target STT path to the Host STT path via hard cutover on the Host OS: load the same Parakeet ONNX model and tokens in-process with `ort` on CPU, using an ONNX Runtime shared library as an app-managed artifact. Remove the Parakeet launcher, HTTP port, and Python sidecar from the Host STT path once the verification gate passes. Keep the STT input contract (mono @ 16 kHz) in the capture path. Setup Wizard / Setup repair and Developer setup consume the updated Platform-module catalog. Soft-fail leaves the Core agent up and emits maximum Host-debuggable diagnostics. Guest OS stays compile-safe with graceful STT degrade — not full STT parity in this iteration.

## User Stories

1. As an end user on the Host OS, I want mic speech transcribed to text without a separate Parakeet server process, so that Voice input feels like part of the app rather than a sidecar that can die on a port.
2. As an end user on the Host OS, I want the same Parakeet model quality I have today after the cutover, so that switching runtimes does not surprise me with worse greedy transcripts.
3. As an end user, I want STT to stay on CPU, so that GPU/VRAM remain available for the chat model.
4. As an end user of the Shipped app, I want the Setup Wizard to download and verify the ONNX Runtime library along with the STT model and tokens, so that I do not need a system-installed ONNX Runtime.
5. As an end user, I want Host STT readiness to mean model + tokens + ONNX Runtime library are present, so that “ready” matches what the in-process engine actually needs.
6. As an end user, I do not want the wizard to require a Parakeet launcher after cutover, so that Setup repair is not asking for an obsolete binary.
7. As an end user, I want Setup repair to restore missing STT app-managed artifacts without wiping memory, so that fixing Voice does not reset the Core agent.
8. As an end user, I want the Core agent to keep working when STT artifacts are missing, so that a Voice problem never bricks chat and memory.
9. As an end user, I want the Core agent to keep working when STT artifacts are present but the engine fails to load, so that a soft STT failure is recoverable.
10. As an end user, I want a clear Setup repair entry when STT soft-fails, so that I know how to recover without hunting logs first.
11. As an end user (and Host developer), I want soft STT failures to emit maximum Host-debuggable diagnostics, so that the next Host iteration can be prompted and fixed from real signal.
12. As an end user, I want transcription to stay off when the STT engine is unavailable, so that I am not fed empty or misleading transcripts.
13. As an end user, I want mic waveform/debug capture to remain usable even when transcription is off (when that already works today), so that I can still see that the mic path is alive while repairing STT.
14. As an end user on first launch, I want the wizard to treat ONNX Runtime as an ordinary app-managed artifact with progress and success/failure, so that STT setup matches other downloads.
15. As an end user, I want re-downloading STT artifacts to be idempotent, so that Setup repair is safe to retry.
16. As an end user on a Guest OS, I want the app to start and keep the Core agent usable when STT is not ready, so that Host-only STT work does not crash Guest opens.
17. As an end user on a Guest OS, I want missing STT to show as not-ready / repairable rather than a hard failure, so that Voice remains optional relative to the Core agent there.
18. As a developer on the Host OS, I want Developer setup to fetch the ONNX Runtime library from the same Platform-module catalog as the wizard, so that URLs and verify rules do not drift.
19. As a developer, I want the Parakeet launcher removed from the Host Platform catalog’s STT requirements, so that “ready?” cannot lie by checking a dead path.
20. As a developer, I want Port 5092 and Parakeet health rows gone from Host monitoring after cutover, so that debug UI matches the real architecture.
21. As a developer, I want in-process STT initialization to use only the CPU execution provider, so that Voice cannot steal GPU from llama.
22. As a developer, I want the voice loop to call a synchronous in-process transcribe behind a blocking boundary, so that HTTP multipart STT disappears from the Host path.
23. As a developer, I want mel spectrogram preprocessing to match the Python/`onnx-asr` reference within ±1e-4 on fixture PCM, so that the main technical unknown is locked before deleting the sidecar.
24. As a developer, I want fixture WAV transcript parity between `ort` greedy CTC and the current sidecar, so that cutover is not gated on “sounds fine on my mic” alone.
25. As a developer, I want a small checked-in audio fixture set for those parity tests, so that CI/agents can verify without a live microphone.
26. As a developer, I want greedy CTC decoding only in this iteration, so that we do not pretend prompt/temperature/LM biasing is in scope.
27. As a developer, I want face/space-style quality work deferred, so that decoder biasing does not block the runtime cutover.
28. As a developer, I want the STT input contract to remain mono PCM at 16 kHz, so that model expectations stay stable across the cutover.
29. As a developer, I want device format negotiation when a capture session starts or restarts, so that conversion uses a known rate/channel layout without re-probing every utterance.
30. As a developer, I want each utterance converted with that session’s negotiated format, so that STT always sees contract audio when resampling succeeds.
31. As a developer, I do not want a Setup Wizard sample-rate tuning step, so that the wizard stays an artifact/repair surface only.
32. As a developer, I want Platform-module isolation preserved for Guest OS stubs, so that Host `ort` work does not bake Host-only assumptions into shared core beyond necessary seams.
33. As a developer opening the repo on a Guest OS, I want compilation to succeed without a working Guest STT engine, so that modular OS structure remains the rule.
34. As a maintainer, I want hard cutover (no dual Host STT path), so that Setup repair never has to explain launcher-or-ort.
35. As a maintainer, I want the sidecar deleted only after the ADR verification gate passes, so that we do not strand Host Voice mid-migration.
36. As a maintainer, I want ADR 0001 respected as the cutover decision record, so that expand–contract is not re-litigated in implementation.
37. As a maintainer, I want TTS/Piper left unchanged, so that spoken output stays out of this iteration’s done bar.
38. As a maintainer, I want the LLM GPU offload slider left out, so that VRAM UX is a separate follow-on.
39. As a QA user on the Host OS, I want to delete the ONNX Runtime library and reopen Setup repair, so that I can verify detection and restore without memory wipe.
40. As a QA user on the Host OS, I want to confirm main UI still appears when Core agent requirements are met but STT is not ready, so that STT is not a false Core gate.
41. As a QA user, I want to confirm SidecarHealth / monitor no longer shows a Parakeet/:5092 row after cutover, so that obsolete health checks are gone.
42. As a QA user, I want one live mic smoke after automated gates pass, so that end-to-end Host Voice still works — without treating smoke as the only proof.
43. As an agent implementing this spec, I want Host STT readiness tests at the existing setup/catalog status seam, so that artifact rules are locked without UI e2e.
44. As an agent, I want mel and transcript parity tests at the in-process STT engine seam, so that inference correctness is proven with fixtures.
45. As an agent, I want soft-fail diagnostics covered where practical at the status/debug seam, so that “maximum Host debug” is not an empty slogan.
46. As an agent, I want capture format preference tests touched only if the cutover risks regressing the STT input contract, so that we do not invent a third product seam.
47. As an end user, I want failed STT artifact downloads to leave previously good artifacts in place when possible, so that a partial failure does not destroy a working Voice install.
48. As an end user, I want Shipped-app recovery copy to point at Setup Wizard / Setup repair, not Developer setup, so that end users are not sent into the toolchain.
49. As a developer, I want `load-dynamic`-style loading of the app-managed ONNX Runtime library from a known app path, so that missing ORT soft-fails instead of preventing process start.
50. As a maintainer, I want Guest STT full parity explicitly out of the done bar, so that this iteration stays a Host cutover.

## Implementation Decisions

- **ADR:** Follow `docs/adr/0001-host-stt-ort-hard-cutover.md`. Hard cutover only; no expand–contract dual Host STT path.
- **Host OS:** Linux is the verification Host. Guest OS = compile-safe Platform-module isolation and graceful STT degrade; not full `ort` STT parity in this done bar.
- **Promotion:** Target STT path (`ort` in-process) becomes the Host STT path when the verification gate passes; Parakeet HTTP sidecar / managed launcher / fixed STT port are removed from the Host path.
- **Model assets:** Reuse the existing Parakeet ONNX model and tokens app-managed artifacts; no new default speech model.
- **ONNX Runtime:** Ship as an app-managed artifact via Platform-module catalog, Setup Wizard, and Developer setup. Not a system prerequisite. Included in Host STT readiness.
- **Host STT readiness:** Model + tokens + ONNX Runtime library. Parakeet launcher removed from ready rules and wizard/catalog STT requirements.
- **Inference:** CPU execution provider only. Greedy CTC decode matching today’s effective sidecar behaviour. No prompt, temperature, or LM biasing in this iteration.
- **Engine seam:** Introduce/replace the Host STT client as an in-process engine (load session + transcribe PCM under the STT input contract). Voice loop invokes transcription off the async executor via a blocking boundary; no HTTP multipart to a local STT port.
- **STT input contract:** Unchanged product contract — mono PCM at 16 kHz. Negotiate device format on capture session start/restart; convert each utterance with that session’s rate/channels. Not a Setup Wizard concern.
- **Failure modes:** Missing/broken artifacts → Setup repair. Artifacts present but engine load fails → soft-fail: Core agent up, transcription off, clear repair entry, **maximum Host-debuggable diagnostics**.
- **Orchestrators:** Setup Wizard and Developer setup remain separate; both consume the updated Platform-module catalog (drop launcher STT requirement; add ORT lib).
- **Monitoring/UI cleanup:** Remove Parakeet/:5092 health from Host debug/monitor surfaces after cutover.
- **Vocabulary:** Use CONTEXT.md terms (Host STT path, Target STT path, STT input contract, app-managed artifact, Setup Wizard, Setup repair, Platform module, Core agent, Host OS, Guest OS).
- **Primary test seams (confirmed):** (1) Setup / Platform catalog status seam — readiness and artifact verify fixtures; (2) In-process STT engine seam — mel ±1e-4 and fixture WAV transcript parity. Supporting: capture format preference only if cutover risks STT input contract regression.

## Testing Decisions

- **Good tests** assert external behaviour at the confirmed seams: given fixture directories or fixture audio, readiness flags, catalog verify results, mel tensors, and transcripts match expectations. Do not assert React wizard internals, live-mic e2e, or full Developer setup live network runs as the primary suite.
- **Setup / catalog seam:** Host STT ready requires model + tokens + ONNX Runtime lib; launcher absence must not be required; Core agent ready remains independent of STT; soft-fail / diagnostic visibility covered where the existing status/debug surface allows.
- **STT engine seam:** Mel spectrogram vs Python/`onnx-asr` reference within ±1e-4 on fixture PCM; greedy CTC transcript parity vs current sidecar on a small checked-in WAV set (exact or agreed normalization). CPU-only provider configuration is part of the engine contract under test where observable.
- **Capture supporting tests:** Reuse existing format-preference coverage only if needed to prevent STT input contract regression.
- **Cleanup checks:** After cutover, Host path must not advertise Parakeet launcher readiness or `:5092` health as the live STT dependency.
- **Manual Host mic smoke:** Necessary after automated gates; not sufficient alone.
- **Prior art:** Fixture-based Rust setup/status and Platform catalog tests from the Setup Wizard Platform SSOT work; existing audio unit tests (capture preference, TTS resample). Prefer the same fixture style for the new engine seam.
- **Guest compile:** Guest Platform modules must continue to compile without assuming a working Guest STT engine.

## Out of Scope

- Expand–contract dual Host STT path (sidecar kept as fallback beside `ort`)
- Decoder LM biasing, prompt/context conditioning, temperature, or dedicated face/space quality work
- TTS / Piper product changes; making spoken output part of Host done
- LLM GPU layer-offload slider (`-ngl`) / Models-tab VRAM UX
- Setup Wizard mic sample-rate tuning or device-optimization UX
- Full Guest OS STT parity (Windows/macOS working `ort` as a done requirement)
- New default speech model unrelated to the current Parakeet ONNX + tokens
- Unifying Developer setup and Setup Wizard into one pipeline
- Memory wipe as a Setup repair default

## Further Notes

- Design narrative and implementation sketch remain in `STT_ORT_MIGRATION.md`; behavioural locks for this iteration are this spec + ADR 0001 + CONTEXT.md.
- After cutover, glossary should treat the live Host STT path as in-process `ort` (Target STT path language may be retired or narrowed in a follow-up glossary pass).
- Face/space / biasing remains an explicit follow-up once Host STT is `ort`.
- Issue tracker path: `.scratch/host-stt-ort-cutover/` (this file is `spec.md`).

Status: done

# Spec: Setup Wizard Platform SSOT

## Problem Statement

Setting up Proactive Agent across operating systems is painful and duplicated. The Setup Wizard is mostly a first-run gate, while Developer setup (`deno task setup` and OS fetch scripts) maintains a parallel list of what to download. Artifact names, URLs, and “ready?” rules drift between those paths. After a successful setup, there is no clear Setup repair entry — users must stumble back in by clearing the chat model or losing binaries. System prerequisite handling is inconsistent (some paths silently install OS runtimes). Meanwhile the Host OS (Linux) already has a working Core agent and Host STT path; the wizard must not become a vehicle for new product features (TTS, Target STT / `ort`), and Guest OS builds must stay modular so opening the project elsewhere does not crash the shared core.

## Solution

Treat the Setup Wizard as a setup/repair surface only: always re-openable for Setup repair without wiping memory. It installs and verifies app-managed artifacts and detects system prerequisites (with install guidance, not package-manager automation as the model). Introduce Platform modules as the single source of truth for per-OS artifact definitions; Developer setup and the Setup Wizard remain separate orchestrators but both consume that contract. On the Host OS (Linux), Core agent + STT (mic → text) remain out-of-box musts. Piper/TTS stays out of the Host completion bar for this work. Guest OS Platform modules compile and register without breaking shared core behaviour.

## User Stories

1. As an end user of the Shipped app, I want to open the Setup Wizard after setup is already complete, so that I can repair missing or broken app-managed artifacts without reinstalling the whole application.
2. As an end user, I want Setup repair to leave my memory and unrelated config intact, so that fixing sidecars does not feel like a factory reset.
3. As an end user, I want memory wipe to remain a separate explicit action (e.g. chat `RESET`), so that the wizard never surprises me by deleting conversation history.
4. As an end user on first launch, I want the Setup Wizard to guide me through downloading required app-managed artifacts, so that I can use the Core agent without a developer toolchain.
5. As an end user, I want the wizard to tell me which system prerequisites are missing and how I might install them, so that I am not stuck on a silent failure.
6. As an end user, I want the wizard not to elevate or run a general OS package manager for me, so that system changes stay under my control.
7. As an end user on Linux (Host OS), I want chat and memory to work after wizard setup, so that the Core agent is usable out of the box.
8. As an end user on Linux (Host OS), I want mic input to produce text via the Host STT path after relevant artifacts are present, so that voice typing works without waiting for Target STT (`ort`).
9. As an end user, I want the Core agent to keep working if Voice/STT is unavailable, so that a mic or sidecar problem does not brick the app.
10. As an end user on a Guest OS, I want the app to start and show clear “not ready / open Setup Wizard” states for missing artifacts, so that I am not greeted with a crash.
11. As an end user, I want missing STT to be explained as repairable, so that I know Voice is optional relative to the Core agent on Guest OSes.
12. As an end user, I do not want the wizard to force TTS/Piper as a hard gate for “setup complete,” so that Host “done” matches mic→text, not spoken replies.
13. As an end user, I want to skip or defer Voice-related downloads when I only need text chat, so that I can reach the Core agent faster (without disabling Host STT as a product must when artifacts are intended to be present).
14. As an end user, I want download progress and clear success/failure for each app-managed artifact, so that I can tell what failed.
15. As an end user, I want re-running a download step to be safe if files already exist, so that Setup repair is idempotent.
16. As an end user, I want to change or load a chat model from the wizard without losing other artifacts, so that model choice is independent of sidecar repair.
17. As a developer on the Host OS, I want `deno task setup` to remain a Developer setup entry point, so that I can prepare a build machine without pretending to be an end user.
18. As a developer, I want Developer setup and the Setup Wizard to read the same Platform-module artifact definitions, so that URLs and filenames do not drift.
19. As a developer, I want per-OS knowledge isolated in Platform modules, so that I can debug Linux setup without spelunking Windows/macOS conditionals in shared core.
20. As a developer opening the repo on a Guest OS, I want the project to compile with Guest Platform modules present, so that shared core does not assume Host-only paths.
21. As a developer, I want Host OS priority (Linux first) documented in behaviour, so that agents working on this machine complete Linux out-of-box before chasing full Guest parity.
22. As a developer, I want the wizard copy and error strings to avoid “run deno task setup” as the primary Shipped-app recovery hint, so that end users are not sent into the development toolchain.
23. As a developer, I want status flags from the setup command surface to drive both first-run and Setup repair UI, so that gate logic stays consistent.
24. As a developer, I want `check_system_deps` (or equivalent) to return structured platform-aware prerequisite results, so that UI can show Host-relevant rows only.
25. As a developer, I want Vulkan/GPU driver absence reported as a system prerequisite (with guidance), so that llama GPU acceleration failures are diagnosable without the wizard installing drivers.
26. As a developer on Windows Guest paths, I want VCRedist handled consistently with “detect + suggest” (or explicitly called out if a narrow exception remains), so that prerequisite policy is not silent and surprising.
27. As a maintainer, I want this work not to implement Target STT (`ort`), so that the migration stays on its own documented track.
28. As a maintainer, I want this work not to design or ship TTS architecture through the wizard, so that Piper remains WIP/out of Host completion bar.
29. As a maintainer, I want existing Host STT behaviour on Linux preserved (auto-start / launcher discovery as today), so that “already working” mic→text does not regress.
30. As a maintainer, I want tests at the Rust setup command surface, so that status, verify, and prerequisite reporting are locked without brittle UI e2e.
31. As a maintainer, I want artifact catalog tests so the same definition yields the same “ready?” answer for wizard and Developer setup consumers.
32. As a QA user, I want to delete one app-managed artifact and reopen Setup repair, so that I can verify the wizard detects and restores it without wiping memory.
33. As a QA user, I want to confirm the main UI appears when Core agent requirements are met even if TTS binaries are absent, so that Piper is not a false gate.
34. As a QA user on Linux, I want to confirm STT model/launcher readiness is visible in setup status, so that Host STT out-of-box is checkable from the same seam as the wizard.
35. As an agent implementing this spec, I want Platform modules to own fetch sources, destinations, and verify rules per OS, so that shared orchestrators stay thin.
36. As an agent, I want Guest Platform modules to be selectable/compilable without executing Host-only install side effects, so that CI or cross-checks do not explode.
37. As an end user, I want a visible entry point (settings, menu, or equivalent) labelled for setup/repair, so that I do not need to clear the chat model to reopen the wizard.
38. As an end user, I want the wizard to re-check prerequisites every time I open Setup repair, so that advice stays current after I install system packages myself.
39. As an end user, I want failed downloads to leave previous good artifacts in place when possible, so that a partial failure does not destroy a working install.
40. As a developer, I want scripts under Developer setup to become thin consumers of the artifact contract (or generated from it), so that shell/PowerShell stop owning divergent URL lists.

## Implementation Decisions

- **Scope fence:** Setup Wizard UX (including Setup repair) + Platform-module artifact SSOT only. No new product features delivered through the wizard (no TTS feature work, no `ort` / Target STT implementation, no new Core agent behaviours).
- **Host OS:** Linux for this effort. Full out-of-box = Core agent + Host STT (mic → text). Guest OSes = modular, compile-safe, non-crashing; parity when that OS becomes Host.
- **Orchestrators stay separate:** Developer setup (Deno/CLI) and Setup Wizard (Shipped app) remain two front doors. They share Platform-module contracts, not one merged pipeline.
- **Platform modules:** Per-OS units holding artifact definitions (names, fetch sources, destinations, verify rules). Shared core must not embed Host-only internals. Today’s duplicated knowledge in in-app download/`#[cfg]` platform bits, constants, and `fetch-sidecars-*` scripts should converge on this contract.
- **Primary test/implementation seam:** Rust setup command surface used by the wizard — setup status, binary/model readiness checks, download-required commands, system dependency check. Prefer extending/clarifying this seam over new UI-only logic.
- **Setup gate alignment:** “Ready for main UI” must reflect Core agent needs (chat model + inference sidecar readiness as required today for chat). Piper/TTS must not remain a hard completion gate for this iteration’s Host bar. STT readiness should be visible in status for Host verification without making Guest absence a crash.
- **Setup repair entry:** Add an explicit user-accessible way to reopen the Setup Wizard after `ready`, without clearing memory. Forced re-entry via missing artifacts may remain as a safety net.
- **App-managed artifacts:** Wizard downloads/verifies only what the app owns under writable app/project data locations (sidecars, libs beside them, embed/STT model files as today, chat model selection). Idempotent re-run.
- **System prerequisites:** Detect and report (platform-aware); may suggest install commands/links. Do not treat silent OS runtime installers as the default policy. If Windows VCRedist retain-an-exception is kept temporarily, document it as an explicit exception to the glossary rule — prefer detect+suggest alignment.
- **Host STT path:** Preserve current Linux Parakeet sidecar/launcher behaviour; wizard/status should support verifying and repairing the app-managed pieces of that path without requiring Target STT.
- **Messaging:** Shipped-app errors should point users at Setup Wizard / Setup repair, not Developer setup, as the primary recovery path.
- **Vocabulary:** Use CONTEXT.md terms throughout code comments and UI copy where user-facing (Setup Wizard, Setup repair, Host OS, Guest OS, Platform module, app-managed artifact, system prerequisite, Core agent, Host STT path, Target STT path).

## Testing Decisions

- **Good tests** assert external behaviour at the setup command / Platform-module contract seam: given fixture directories or fake HTTP, status flags, file placement, idempotency, and prerequisite reports match expectations. Do not assert React component internals or full `deno task setup` live network runs as the primary suite.
- **Modules under test:** setup status and readiness reporting; app-managed artifact download/verify against Platform-module definitions; system prerequisite reporting; sidecar presence rules that protect Host STT launchers (e.g. small Unix executable vs Windows size gate) as a narrow supporting seam.
- **Prior art:** Rust `#[cfg(test)]` unit tests exist under the audio/TTS area; prefer the same style (pure/fixture-based) for the setup seam. No existing wizard/setup tests — this work establishes them at the command/catalog seam.
- **Regression bar (Host):** After changes, Linux Core agent chat path and existing Host STT mic→text path must still function when artifacts are present; absence of Piper must not block main UI if Core requirements are met.
- **Guest compile:** Non-host Platform modules must not break shared-core compilation.

## Out of Scope

- Implementing Target STT (`ort`) or deleting the sidecar architecture beyond what status/wizard already imply
- TTS / Piper product design, wizard-driven TTS feature delivery, or making spoken output part of Host “done”
- Unifying Developer setup and Setup Wizard into a single runtime pipeline
- Full verified voice/GPU parity on Windows and macOS in this iteration
- Proactivity persistence across restarts, GPU layer-offload slider, installer MSI polish, and other ROADMAP items unrelated to wizard/Platform SSOT
- Using the wizard to ship unrelated new Core agent features
- Memory wipe / full factory reset as a wizard default (optional future nice-to-have only)

## Further Notes

- Long-term STT direction remains documented in `STT_ORT_MIGRATION.md` (Target STT path); this spec intentionally keeps Host STT path for the iteration.
- Architecture already locks GPU/VRAM for the chat model and CPU for Voice workloads; do not reverse that.
- Optional follow-up ADRs (not required to start implementation): (1) Host STT this iteration vs Target STT later; (2) separate orchestrators + Platform-module SSOT.
- Issue tracker path for this feature: `.scratch/setup-wizard-platform-ssot/` (this file is `spec.md`).

## Implemented work summary

All issues completed:

- **01:** Core gate without piper; stt_ready; fixture tests
- **02:** Platform modules linux+guests; verify API; tests
- **03:** Structured prerequisites detect+suggest
- **04:** Wizard uses catalog for model downloads; recovery copy
- **05:** Setup repair nav entry; no memory wipe
- **06:** Host STT visible in status/wizard
- **07:** linux.json + fetch script consumes catalog
- **08:** scripts/binary_store use catalog as SSOT for URLs/patterns

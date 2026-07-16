# Proactive Agent

Fully local, voice-enabled desktop assistant with persistent memory and proactive follow-ups. No cloud required for the core path.

## Language

### Product

**Proactive Agent**:
The local desktop assistant product: chat, memory, mic voice input (STT), and scheduled follow-ups on the user's machine. Spoken output (TTS) is aspirational/WIP.
_Avoid_: chatbot, copilot, cloud agent

### Platforms

**Host OS**:
The operating system where this repo is currently being developed and primarily verified. Full out-of-box behaviour is required here first.
_Avoid_: primary OS, target OS (ambiguous with end-user machines)

**Guest OS**:
Any supported OS that is not the current Host OS. It must stay modular and compile-safe so opening the project there does not break shared core behaviour; full parity is required when that OS becomes the Host OS.
_Avoid_: secondary OS, unsupported OS

**Platform module**:
The isolated, per-OS unit of code and assets for setup and runtime differences. Shared core must not depend on one OS's internals. Holds the single source of truth for that OS’s app-managed artifact definitions (names, fetch sources, destinations, verify rules) consumed by both Developer setup and the Setup Wizard.
_Avoid_: ifdefs sprinkled through shared code, platform soup, duplicated URL lists per tool

### Setup

**Setup Wizard**:
The in-app flow that installs and verifies **app-managed artifacts** (sidecars, bundled runtime libs, models, STT assets under app data dirs) and that can report missing **system prerequisites**. It is a setup/repair surface only — it does not introduce new product features or capabilities.
_Avoid_: installer (the OS package/MSI), setup script (CLI-only path), feature delivery vehicle

**App-managed artifact**:
A binary, library, model, or data tree the application downloads and places under its own writable locations (project `binaries/`/`models/` in dev, or OS app-data in release).
_Avoid_: dependency (too broad), system package

**System prerequisite**:
An OS- or toolchain-level requirement the app does not install (e.g. WebView/WebKit, GPU drivers; for developers Rust/Deno). The Setup Wizard detects presence/absence and may explain how to install what’s missing; it does not elevate or run a package manager for the user.
_Avoid_: dependency, sidecar

**Setup repair**:
Re-opening the Setup Wizard after a prior successful setup to re-check prerequisites and app-managed artifacts and fix what’s missing or broken, without wiping memory or unrelated config. Memory wipe stays a separate user action (e.g. chat `RESET`), not a wizard default. This iteration’s structural work is Setup repair UX plus Platform-module artifact single source of truth — not new product features.
_Avoid_: full reset (as the default wizard re-entry), first-run only, shipping features through the wizard

**Developer setup**:
The CLI/dev-toolchain path (`deno task setup` and related scripts) that prepares a development machine to build and run the project. It is not the end-user product surface and may assume Deno, Rust, and other system prerequisites.
_Avoid_: Setup Wizard, installer

**Shipped app**:
The standalone Tauri binary and the services/sidecars it runs for end users. It must not require Deno or a Rust toolchain on the user’s machine.
_Avoid_: development build, deno task

### Voice

**Host STT path**:
The speech-to-text integration used on the current Host OS for this iteration (today: Parakeet HTTP sidecar / managed launcher). Out-of-box voice input on the Host OS means this path works via Setup Wizard / Setup repair, not a blocked wait on the long-term replacement.
_Avoid_: Whisper (retired)

**Target STT path**:
The planned in-process ONNX runtime (`ort`) replacement for the sidecar, documented in `STT_ORT_MIGRATION.md`. It is the future cross-OS STT approach; it is not the completion bar for this OS-compatibility iteration.
_Avoid_: current sidecar, Python STT as the end state

**Core agent**:
The product’s primary capability: local chat, memory, and proactive follow-ups. It must work on the Host OS even when voice input or output is unavailable.
_Avoid_: voice app, STT-first product

**Voice**:
Speech capabilities around the Core agent. On the Host OS, **STT** (mic → text) is part of the out-of-box completion bar when that path is already proven. **TTS** (spoken replies) is future/WIP — not required for this iteration’s wizard or Host “done” bar; its architecture may still change. Voice workloads stay on CPU so GPU/VRAM remain for the chat model. On a Guest OS, missing Voice must degrade gracefully — never take down the Core agent.
_Avoid_: TTV, requiring GPU for STT/TTS, treating TTS as done

# 11 — Derive Platform readiness for Setup

**What to build:** “Core agent ready” and “Host STT ready” in setup status come from the Platform catalog’s verify results and `required_for_*` flags (chat model presence still required for Core). The Setup Wizard / Setup repair present that derived status instead of computing a second, parallel ready answer. Catalog rows that are not required do not falsely gate Core. No change to what the wizard downloads beyond aligning readiness with the catalog.

**Blocked by:** None — can start immediately (prefer after ticket 10).

**Status:** resolved

- [x] Setup status Core / Host STT gates derive from catalog verify + `required_for_core` / `required_for_stt`, plus chat model for Core
- [x] FE / wizard readiness uses that derived status — no unused parallel “ready?” answer for the same gates
- [x] Wizard remains setup/repair only; Piper/TTS stays out of the Host completion bar
- [x] Guest Platform modules still compile with the same derivation pattern
- [x] Seam tests: same layout roots → status gates match required catalog artifacts (including Host STT = encoder + decoder + vocab + ORT)
- [x] ADR 0001 honored: no Parakeet HTTP sidecar; soft-fail / repair entry for Host STT remains meaningful

## Answer

`derive_setup_status(artifacts, LayoutRoots, chat_model, data_dir)` is the Platform readiness seam: Core = chat model + all `required_for_core` verify-ready; Host STT = all `required_for_stt` verify-ready. `build_setup_status` / `check_binaries_in` adapt the current catalog; the Setup Wizard refreshes a single live `SetupStatus` from `get_setup_status` only. Seam tests cover flag-driven catalogs, Host linux catalog (encoder+decoder+vocab+ORT), non-required rows not gating Core, and Guest windows/macos derivation.

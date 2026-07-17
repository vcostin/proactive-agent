# SetupStatus vs Platform verify_catalog — current duplication

Type: research
Status: resolved
Blocked by:

## Question

Against this repo’s source (primary): how does `setup/status` / `SetupStatus` / wizard readiness currently relate to `platform` `verify_catalog` / `artifact_ready` / `required_for_core` / `required_for_stt`?

Document: which fields or gates are computed twice, which catalog verify results are unused by the wizard, overlapping ORT/sidecar path helpers, and concrete file:line evidence. Facts only — no redesign.

## Answer

Setup status and Platform verify are parallel seams: the wizard/`App` only consume `get_setup_status` (`SetupStatus`); `verify_catalog` / `required_for_*` are never read by the FE or by `setup/`. Core/STT formulas overlap semantically (and match ADR 0001’s Host STT list) but are implemented twice with overlapping sidecar/ORT path helpers. Full citations: [research-setup-status-vs-verify-catalog.md](../research-setup-status-vs-verify-catalog.md).

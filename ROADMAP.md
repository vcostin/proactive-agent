# Proactive Agent — Roadmap

**Current state:** Chat, streaming, memory, voice in/out, and debug panel work on Linux
(Deno) and Windows. Parakeet STT auto-starts on Linux; TTS via Piper. Inference and
memory are solid — the growth path below turns that stack into a durable, product-shaped
agent.

---

## Growth backlog (ordered)

Work top → bottom. Each phase stands alone and ships value without waiting on later phases.

### 1. Proactivity that survives restarts — **P0 product**

Makes the “proactive” half of the name real: the agent can come back after you leave.

| # | Task | Notes |
|---|------|-------|
| 1.1 | Persist deferred queue | Save pending `<defer>` jobs to disk (JSON next to config or LanceDB); reload on launch |
| 1.2 | Survive clock skew / overdue | On startup, fire or re-schedule past-due items instead of dropping them |
| 1.3 | TTS on proactive nudges | When voice output is on, speak scheduled messages (same Piper path as chat replies) |
| 1.4 | Dedup / cancel API | Avoid stacking identical follow-ups; allow cancel from Debug → Scheduler |
| 1.5 | Persona nudge for deferral | Light prompt guidance so the model actually emits `<defer>` for useful follow-ups |

**Done when:** close the app with a pending defer → reopen later → message appears (and is spoken if TTS is on).

---

### 2. STT → in-process `ort` — **P1 infrastructure**

Replace the Parakeet HTTP sidecar (Windows frozen exe / Linux managed Python) with CPU
ONNX via the `ort` crate. Full plan: [`STT_ORT_MIGRATION.md`](STT_ORT_MIGRATION.md).

**Hardware (locked):** GPU VRAM → LLM only · CPU → STT / TTS / embeddings / LanceDB.

| # | Task | Notes |
|---|------|-------|
| 2.1 | Add `ort` + `rustfft` | `Cargo.toml`; dynamic ORT load |
| 2.2 | `log_mel_spectrogram()` | Verify against current Python / sidecar output |
| 2.3 | Greedy CTC decode | Use existing `tokens.txt` from wizard download |
| 2.4 | Rewrite `SttClient` | Hold `ort::Session`; drop `reqwest` for STT |
| 2.5 | Wire `run_stt_loop` | `spawn_blocking` for inference |
| 2.6 | App-state init | Load session once at startup |
| 2.7 | Remove sidecar surface | Monitor health row, `SidecarHealth.tsx`, Linux launcher / setup path |
| 2.8 | Acceptance | Accent check; confirm ~0 VRAM used by STT |

**Done when:** mic works with no process on `:5092` and no `.cache/parakeet-tdt` required.

*(Checklist item “empty `externalBin`” already done.)*

---

### 3. GPU layer offload slider — **P2 ergonomics**

Expose llama-server `-ngl N` in the Models tab.

| # | Task | Notes |
|---|------|-------|
| 3.1 | Config field | Persist `ngl` (default `999`) in `AppConfig` / config.json |
| 3.2 | Models UI slider | `0` (CPU) ← → `999` (all layers GPU) |
| 3.3 | Restart chat server | Pass updated `-ngl` on hot-swap / apply |

**Done when:** user can fit a larger GGUF by dialing layers down without editing CLI flags.

---

### 4. Platform & polish — **P3**

| # | Task | Notes |
|---|------|-------|
| 4.1 | macOS bring-up | Unblocked after `ort`; exercise existing fetch script on M-series |
| 4.2 | Installer gaps (Windows) | Bundle `espeak-ng-data/`, `ggml-cpu-*.dll`, `libomp140` |
| 4.3 | Cleanup | Real VCRedist SHA256; route capture errors to debug log; VAD logs debug-only |
| 4.4 | `tauri-specta` | Typed invoke/event IPC when command surface stabilises |
| 4.5 | Vitest smoke tests | After UI stops churning |

---

## ✅ Done (summary)

Voice stack, LanceDB memory + semantic distillation, `<defer>` scheduler (in-memory),
model hot-swap, Deno tooling, Linux Parakeet auto-start, empty `externalBin`, security
hardening, debug panel. Full archaeological detail: `WORK_LOG.md`.

---

## Docs

| File | Purpose |
|------|---------|
| `ARCHITECTURE.md` | Current system — ports, binaries, decisions |
| `SETUP.md` | Install and run |
| `STT_ORT_MIGRATION.md` | Detail for backlog §2 |
| `WORK_LOG.md` | Bug history and rationale |
| `CONTRIBUTING.md` | Branch → test → merge |
| `SUPERVISOR.md` | AI review checklist |

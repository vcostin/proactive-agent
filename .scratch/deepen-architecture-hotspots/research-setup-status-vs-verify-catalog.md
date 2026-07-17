# Research: SetupStatus vs Platform verify_catalog

Question: how does `setup/status` / `SetupStatus` / wizard readiness currently relate to `platform` `verify_catalog` / `artifact_ready` / `required_for_core` / `required_for_stt`?

Primary sources: this repo’s source + ADR 0001 (readiness language as context only). Facts only — no redesign.

---

## 1. Two parallel seams (no shared call graph)

| Seam | Entry | Output | Frontend use |
|------|--------|--------|----------------|
| Setup status | `get_setup_status` → `setup::build_setup_status` | `SetupStatus` | `App.tsx`, `SetupWizard.tsx` |
| Platform verify | `verify_platform_artifacts` → `platform::verify_catalog` | `Vec<VerifyStatus>` | **none** (command registered only) |
| Catalog projection | `get_artifact_catalog` → `project_catalog` | `CatalogProjection` (includes `required_for_*`) | **none** |

Evidence:

- `get_setup_status` builds status only via `setup::build_setup_status` — no `platform` import (`src-tauri/src/commands.rs` 68–81).
- `verify_platform_artifacts` / `get_artifact_catalog` are separate commands (`src-tauri/src/commands.rs` 84–104); registered in `lib.rs` (192–195).
- Grep of `src/` finds **zero** invokes of `verify_platform_artifacts` or `get_artifact_catalog`. Wizard uses `get_setup_status`, `check_binaries_ready`, `check_system_deps`, download commands (`SetupWizard.tsx` 37–72).
- `src-tauri/src/setup/` has **no** references to `platform::`, `verify_catalog`, or `artifact_ready`.

---

## 2. What each seam computes

### 2.1 SetupProbe / SetupStatus gates

Defined in `src-tauri/src/setup/status.rs`:

- `core_agent_ready` = `chat_model_present && llama_ready` (28–30). Comment: Piper excluded (26–27).
- `host_stt_ready` = `stt_model_ready && stt_vocab_ready && ort_lib_ready` (32–35).
- `stt_model_ready_in` = encoder **and** decoder files under `STT_MODEL_REL_DIR` (71–75).
- `stt_vocab_ready_in` = vocab file (78–82).
- `ort_lib_ready_in` = any file under `binaries/ort/` whose name contains `.so` or ends with `.dll` / `.dylib` (85–96).
- `build_setup_status` maps probe → `SetupStatus` with `ready`, per-field flags, nested `binaries: BinariesStatus` (138–157).

DTO mirror: `src/types.ts` 11–24.

### 2.2 Catalog flags and verify

`ArtifactDef` carries `required_for_core` / `required_for_stt` (`src-tauri/src/platform/artifact.rs` 60–63).

Linux catalog (`src-tauri/src/platform/linux.rs`):

| id | `required_for_core` | `required_for_stt` |
|----|---------------------|--------------------|
| llama-server | true | false |
| llama-vulkan-libs | false | false |
| piper | false | false |
| tts-voice | false | false |
| embed-model | false | false |
| stt-encoder | false | true |
| stt-decoder | false | true |
| stt-vocab | false | true |
| onnxruntime | false | true |

`artifact_ready` / `verify_catalog` (`artifact.rs` 108–147) apply `VerifyRule` per artifact; they **do not read** `required_for_*`. Those flags are only copied into JSON projection (`174–191`).

Grep of `src-tauri`: `required_for_core` / `required_for_stt` appear only in platform catalogs, `ArtifactDef` / projection, and `scripts/artifacts/*.json` — **never** in setup status, binary_store gating logic beyond URL lookup, or frontend.

---

## 3. Semantic overlap vs independent implementation

### Core agent

| Concern | Setup status | Catalog |
|---------|--------------|---------|
| Llama sidecar | `llama_ready` via `find_sidecar_in` | `llama-server` + `SidecarUsable` + `required_for_core: true` |
| Chat model | `chat_model` path exists (config) | **not in catalog** |
| Combined gate | `SetupStatus.ready` | no aggregated “core ready” API |

App UI gate uses only `setupStatus.ready` (`src/App.tsx` 49–51).

### Host STT

| Concern | Setup status | Catalog |
|---------|--------------|---------|
| Encoder + decoder | one bool `stt_model_ready` | two ids: `stt-encoder`, `stt-decoder` |
| Vocab | `stt_vocab_ready` | `stt-vocab` |
| ORT lib | `ort_lib_ready` / `binaries.ort_ready` | `onnxruntime` + `SharedLibPresent` |
| Combined gate | `stt_ready` = `host_stt_ready` | no aggregated API; flags only on defs |

ADR 0001 states the same Host STT formula (encoder + decoder + vocab + ORT lib; Platform catalog / Setup Wizard / Developer setup) as **intent** (`docs/adr/0001-host-stt-ort-hard-cutover.md` lines 11, 21). Current code implements that formula in **setup/status**, not by folding `verify_catalog` + `required_for_stt`.

### Reported but not core/STT-gated

- Piper: `binaries.piper_ready` / catalog `piper` with both required flags false.
- Embed: `embed_model_ready` / catalog `embed-model` with both required flags false.
- TTS voice: catalog `tts-voice` (linux) — **absent** from `SetupStatus`.
- `llama-vulkan-libs`: catalog only; download may use its GithubRelease pattern (`binary_store.rs` 130); not a SetupStatus field.

---

## 4. Fields / gates computed twice (or in parallel)

### 4.1 Same setup path called twice per `build_setup_status`

```138:156:src-tauri/src/setup/status.rs
pub fn build_setup_status(...) -> SetupStatus {
    let probe = probe_layout(...);           // calls check_binaries_in
    let binaries = check_binaries_in(...); // second call
    SetupStatus { ... binaries, ... }
}
```

`probe_layout` already runs `check_binaries_in` (109–114).

### 4.2 Parallel readiness for the same on-disk artifacts

These pairs answer “is X present?” independently:

| Artifact | Setup path | Catalog path |
|----------|------------|--------------|
| llama / piper | `find_sidecar_in` → `sidecar_file_usable` | `LayoutRoots::resolve` + `VerifyRule::SidecarUsable` → same `sidecar_file_usable` |
| STT ONNX + vocab | `stt_model_ready_in` / `stt_vocab_ready_in` (`Exists`-style `is_file`) | `artifact_ready` + `VerifyRule::Exists` |
| ORT dir | `ort_lib_ready_in` | `VerifyRule::SharedLibPresent` (nearly identical directory scan) |
| Embed | `embed_model_ready_in` | `embed-model` Exists |

`binary_store::check_binaries` delegates to setup’s `check_binaries_in` (`binary_store.rs` 78–80) — wizard’s post-download refresh shares setup’s answer for binaries, still not catalog verify.

### 4.3 Wizard step heuristics vs Rust aggregated gates

`SetupWizard` does **not** call `status.ready` or `status.stt_ready` for step progression:

- `initialStep`: `!llama_ready` → tools; else `!embed_model_ready || !stt_model_ready` → models; else chat (`SetupWizard.tsx` 19–23). Omits `stt_vocab_ready` and `ort_ready` / `stt_ready`.
- `toolsDone` = `binaries.llama_ready` (100).
- `modelsDone` = `embed_model_ready && stt_model_ready` (101) — again without vocab.
- `stt_ready` is display-only banners (`sttReady` state, 33–39, 142, 153, 231–237, 350–356).

App-level “must run wizard” uses Rust `ready` only (`App.tsx` 51).

---

## 5. Catalog verify results unused by the wizard

1. Entire `verify_catalog` output (`Vec<VerifyStatus>` with per-id `ready` + `path`) — no FE consumer.
2. `required_for_core` / `required_for_stt` — serialized for Developer/scripts; unused for wizard gates.
3. Per-artifact catalog rows with no SetupStatus counterpart used for gating:
   - `llama-vulkan-libs`
   - `tts-voice`
4. Catalog rows that **overlap** SetupStatus reporting but are still not consumed via verify:
   - `piper`, `embed-model`, `stt-*`, `onnxruntime`, `llama-server` — wizard reads the **setup** DTO / `BinariesStatus`, not verify statuses.

Wizard still **lists** overlapping concerns (llama, piper, ORT, embed, encoder/decoder/vocab) from `SetupStatus` / `BinariesStatus` (`SetupWizard.tsx` 197–221, 308–336).

---

## 6. Overlapping ORT / sidecar path helpers

### Sidecar location

| Helper | Location | Behavior |
|--------|----------|----------|
| `find_sidecar` | `lib.rs` 403–431 | `binaries_dir()` + short/name/flat candidates; release also searches exe dir |
| `find_sidecar_in` | `setup/status.rs` 53–64 | Same short/name/flat under arbitrary root; no exe-dir |
| `LayoutRoots::resolve` (`sidecar_name`) | `artifact.rs` 83–96 | Prefer nested short path if exists, else `base/name/triple`, else return nested (even if missing) |

All three use `sidecar_filename` (`lib.rs` 380–390). Usability gate for setup/lib is `sidecar_file_usable` (`lib.rs` 436–449); catalog sidecars use the same via `SidecarUsable`.

### ORT presence / path

| Helper | Location | Behavior |
|--------|----------|----------|
| `ort_lib_ready_in` | `setup/status.rs` 85–96 | Any file with `.so` substring or `.dll`/`.dylib` suffix |
| `SharedLibPresent` | `artifact.rs` 112–130 | Same extension heuristic under `relative_dir` |
| `resolve_ort_dylib` | `audio/stt.rs` 97–127 | Name-specific: `libonnxruntime.so` / `.so.*` / `.dylib` / `onnxruntime.dll`; picks shortest name |
| `AppConfig::ort_lib_dir` | `config.rs` 128–129 | Path only: `binaries_dir()/ort` |

Download skip uses setup’s `ort_lib_ready_in` (`binary_store.rs` 110–116), not `resolve_ort_dylib` or catalog `artifact_ready`.

Consequence of the name vs extension difference: a non-ORT `.so` in `binaries/ort/` can make setup/catalog report ready while `SttClient::new` still fails resolution — fact of current helpers, not a redesign note.

---

## 7. Command / type surface summary

```
Frontend App / SetupWizard
  └─ get_setup_status → setup::build_setup_status → SetupStatus
  └─ check_binaries_ready → binary_store::check_binaries → setup::check_binaries_in
  └─ (never) verify_platform_artifacts → platform::verify_catalog
  └─ (never) get_artifact_catalog → project_catalog (includes required_for_*)

binary_store::download_all
  └─ find_sidecar / ort_lib_ready_in for skip
  └─ catalog_github(id) only for GithubRelease URL/pattern SSOT
```

---

## 8. ADR 0001 (context only — not current wiring)

ADR 0001 readiness language: Host STT ready = encoder + decoder + vocab + ONNX Runtime lib; launcher gone; surfaces named as “Platform catalog / Setup Wizard / Developer setup” (`docs/adr/0001-host-stt-ort-hard-cutover.md` 11, 21).

Observed in code today: that **formula** matches `host_stt_ready` in setup/status; Platform catalog defines the same pieces with `required_for_stt: true` but does not drive SetupStatus or the wizard.

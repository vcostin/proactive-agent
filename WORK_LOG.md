# Proactive Agent — Work Log

Chronology and hard-won lessons. Current architecture: `ARCHITECTURE.md`. Plans: `ROADMAP.md`.

---

## Session — Linux Deno bring-up + Parakeet auto-start (Jul 2026)

- Deno-first tooling; Linux sidecar fetch (llama tarball, Vulkan soname links, Piper, models)
- Parakeet: managed CPU venv in `.cache/parakeet-tdt/` + small launcher under `binaries/parakeet/`;
  app auto-spawns if `:5092` is down
- **Bug:** `find_sidecar` required `len > 1024` → ignored ~659 B launcher → mic worked, no transcription.
  Fixed: Unix `sidecar_file_usable` (`>32` + exec bit). Mic waits ~25s for STT warm-up.
- `externalBin` cleared. Next: in-process `ort` (`STT_ORT_MIGRATION.md`).

---

## Episodic memory role-blurring bug

**Symptom:** Mid-conversation the LLM swapped roles — answered as if it *were* the user.

**Cause:** Episodic turns stored/retrieved without a `role` label, then injected as an unlabeled
blob. The model could not tell whose words were whose.

**Fix:** Every episodic row has `role` (`User` | `Assistant`); retrieval injects explicit prefixes.
Removing or flattening that field reintroduces the bug — encoded as a hard stop in `SUPERVISOR.md`.

---

## Why Whisper and Kokoro were replaced

- **Whisper:** Accuracy on the developer's voice was unusable (“a disaster”). Retired permanently.
- **Kokoro:** Long-running HTTP TTS replaced by Piper per-utterance subprocess; leftover health check
  showed a permanent red dot until removed.
- Flat `binaries/` mixing llama/whisper DLLs caused `STATUS_DLL_NOT_FOUND` /
  `STATUS_ENTRYPOINT_NOT_FOUND` — fixed by per-sidecar subdirectories.

Bar for any future STT change: usable on the same voice, not just benchmarks.

---

## Wizard-owned binaries (decision)

Listing llama/piper in `externalBin` forced compile-time presence → chicken-and-egg with the
first-run wizard. They were removed from `externalBin` (now **empty**); release downloads go to
AppData / XDG data (`binaries_dir()`), not Program Files.

---

## Key discoveries (short)

| Topic | Lesson |
|-------|--------|
| Port 8080 | LM Studio owned it — silent bind failures. Use **18080–18083**. |
| Vulkan llama-server | Vulkan-only build is `/health`-stripped; use CPU binary + Vulkan libs. |
| Parakeet port | Upstream hardcodes **5092**; config must match until ort removes the port. |

---

## Notable bugs fixed

| Bug | Root cause | Fix |
|-----|-----------|-----|
| DLL / entrypoint crashes | Mixed sidecar DLLs | Per-directory isolation |
| API 404 for weeks | Port 8080 collision | Ports 18080+ |
| Model ID 404 | `/v1/models` shape variance | Parse both `models` and `data` |
| Chat history lost on tab switch | Unmount cleared state | Keep tabs mounted (CSS) |
| TTS silent / 2× speed | Stale closure; wrong rate/channels | `ttsEnabledRef`; resample + stereo upmix |
| cpal `!Send` | WASAPI stream | Dedicated capture thread |
| Linux STT “not found” | Size gate >1024 on launcher | `sidecar_file_usable` |
| Sidecars locked after exit | Processes left running | Kill on `RunEvent::Exit` |

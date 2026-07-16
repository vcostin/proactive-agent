# Host STT hard-cutover to in-process ort

The Host STT path today is the Parakeet HTTP sidecar (Python / managed launcher / `:5092`). When the Target STT path (`ort` in-process) meets this iteration’s verification bar, it becomes the Host STT path in one cut — the sidecar is removed rather than kept as a parallel or fallback path.

Expand–contract (ship both, then delete) was rejected: dual readiness and Setup repair surfaces (launcher *and* ort) delay the real done bar and fight the temporary-sidecar framing already in `STT_ORT_MIGRATION.md`. Merge is gated on mel spectrogram parity and fixture-transcript parity with the current sidecar; until then the sidecar remains the live Host path.

**Verification gate (Host merge / sidecar delete):**
1. Mel unit test: fixture PCM → Rust/ort nemo128 (onnx-asr preprocessor) vs Python/`onnx-asr` reference within ±1e-4.
2. Fixture transcript parity: small checked-in WAV set; `ort` greedy **TDT** matches (or agreed-normalize-matches) current sidecar behaviour on those clips.
3. CPU-only STT execution provider; no STT GPU/VRAM use.
4. Artifact/readiness tests: Host STT ready = encoder + decoder + vocab + ONNX Runtime lib; launcher gone from catalog/ready; soft-fail emits rich Host diagnostics.
5. Cleanup: no `parakeet-server` / `:5092` / SidecarHealth parakeet row / wizard launcher row on the Host path.
6. Manual Host mic smoke is necessary but not sufficient.

Accent/latency checks from exploratory checklists are smoke only, not merge blockers.

**Consequences:** This cutover matches today’s effective decoding (greedy **TDT** on the Parakeet TDT 0.6B v3 export; no `prompt` / temperature / LM biasing). The earlier “CTC” wording in exploratory notes was incorrect relative to the live Host sidecar (`nemo-conformer-tdt`). Face/space-style errors attributed to missing decoder priors are explicitly out of scope — a follow-up after Host STT is `ort`, not a gate on removing the sidecar.

Guest OS is not in this iteration’s STT done bar: compile-safe Platform-module isolation and graceful degrade (Core agent stays up; STT may remain not-ready). Full Guest `ort` parity waits until that OS is Host or is explicitly scheduled. Host `ort` work must stay modular (Platform module / OS-specific seams) so shared core does not assume a working Guest STT path.

**Failure modes (Host):** After cutover, Host STT readiness is app-managed STT artifacts: Parakeet ONNX model + tokens + **ONNX Runtime shared library** (Platform catalog / Setup Wizard / Developer setup — not a system prerequisite). The Parakeet launcher is removed from readiness. Missing/broken artifacts → Setup repair. Files present but `ort` load fails → soft-fail: Core agent stays up, STT/transcription off, clear repair entry. Soft failures must emit **maximum Host-debuggable diagnostics** (enough signal to fix on the Host machine and to prompt follow-up Host iterations) — not a silent or minimal error toast.

**Out of scope for this iteration:** TTS/Piper changes; LLM GPU offload slider (`-ngl`); Setup Wizard sample-rate / mic-tuning UX; decoder LM biasing / prompt / temperature / face-space quality work. Capture keeps the existing STT input contract (session negotiate + per-utterance convert).

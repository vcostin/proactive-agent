# Research: Sound conversion / audio preprocessing for Host STT

**Date:** 2026-07-17  
**Scope:** Best-practice capture → contract audio for short-utterance ASR with Parakeet-class / NeMo TDT models on desktop mics.  
**Stack under study:** in-process Parakeet TDT 0.6b v3 ONNX via `parakeet-rs` + `ort` (CPU), greedy decode; STT input contract mono PCM float @ 16 kHz.

---

## Summary

Parakeet / NeMo TDT models expect **mono, 16 kHz float PCM**; mel features (NeMo-style log-mel / onnx-asr `nemo128`) assume that contract. Our live path already does the right *shape* of conversion: prefer better capture rates → louder-channel mono → band-limited sinc resample → peak-normalize to 0.7. Fixture work on `face_space` shows the remaining failures are mostly **segmentation and level relative to silence**, not missing vocabulary: raw ~2.9 s with long silence pads and peak ~0.18 yields empty; peak-norm 0.7 or energy-trim recovers `Face.`; VAD-only frames can garble (`Fez.`).

Highest leverage for *this* codebase (minimal change first):

1. **Stop hard-gating the mic buffer on RMS alone** — add pre-roll / hangover / post-roll so plosive onsets (`/f/` in *face*) are not chopped.
2. **Trim long leading/trailing silence** (with a small pad) before STT — NeMo training often uses `trim_silence`; long pads push TDT/CTC toward blank-heavy empty hypotheses.
3. **Keep utterance-level peak-norm (~0.7 / −3 dBFS)** and louder-channel downmix — already aligned with observed recovery and with phase-cancellation risk on mono-as-stereo USB mics.
4. **Treat phrase boosting / n-gram LM as orthogonal** — decoder-side; does not replace conversion/VAD fixes (NeMo documents LM fusion separately from audio I/O).

---

## Current pipeline (as implemented)

| Stage | Where | Behavior |
|-------|--------|----------|
| Capture format | `src-tauri/src/audio/capture.rs` (`pick_input_config`) | Prefer mono F32 @ 16 kHz, then mono @ 48/44.1, then stereo F32 @ 48/16; avoid Pulse default 44.1 stereo when possible |
| Device pick | `resolve_input_device` | Prefer `pulse` / `pipewire` PCMs over broken ALSA `default` |
| VAD gate | `AudioCapture::start` / `VadGate` | Per-callback RMS via `frame_rms` (loudest channel); **200 ms pre-roll + hangover** around speech (**shipped**) |
| Utterance end | `src-tauri/src/audio/mod.rs` (`run_stt_loop`) | 1 s silence timeout (`SILENCE_MS`); require ≥ 0.4 s buffered audio |
| Downmix | `prepare_for_stt` | Stereo → **louder of L/R** (energy), not average |
| Resample | `prepare_for_stt` | `rubato::SincFixedIn`, `sinc_len=64`, `f_cutoff=0.95`, Linear interp, oversampling 64, BlackmanHarris2; new resampler per utterance |
| Level | `amplify` | Peak normalize to **0.7**, gain capped at 20×, then clamp to ±1 |
| Silence trim | `trim_silence` | Energy-island trim with ±200 ms pad before amplify (**shipped**) |
| STT | `src-tauri/src/audio/stt.rs` | `ParakeetTDT::transcribe_samples(pcm, rate, channels, None)` — contract expects mono @ `STT_SAMPLE_RATE` (16000) |
| Post-filter | `clean_transcript` | Drops hallucination phrases; keeps short words with **≥2 letters** (**shipped**; was `len < 4`) |

Observed fixture behavior (`src-tauri/tests/fixtures/stt/face_space/`, `baseline.json`, README): raw empty vs `Face.` after peak 0.7 or energy-trim; VAD-tight audio can confuse fricatives.

---

## Findings by topic

### 1. Sample-rate conversion (48 kHz → 16 kHz)

**What the model requires**

- NVIDIA Parakeet TDT 0.6B v3 model card: input is **16 kHz monochannel** `.wav` / `.flac`.  
  Source: <https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3>
- NeMo transcription docs: caller must supply **mono, 16 kHz** before the model; audio files for inference should be 16 kHz mono wav.  
  Source: <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/results.html>
- NeMo ASR configs default `sample_rate: 16000` on datasets.  
  Source: <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/configs.html>
- `parakeet-rs`: **16 kHz mono** (stereo converted automatically); 16-bit PCM or float.  
  Source: <https://docs.rs/parakeet-rs/latest/parakeet_rs/>
- onnx-asr accepts WAV or NumPy with **built-in resampling**; Parakeet via `nemo-parakeet-tdt-0.6b-v3`.  
  Source: <https://github.com/istupakov/onnx-asr>, <https://istupakov.github.io/onnx-asr/usage/>

**Sinc vs linear vs polyphase / anti-alias**

- Downsampling without a low-pass near the new Nyquist (8 kHz for 16 kHz audio) **aliases** high-frequency energy into the speech band — hurts consonants (`s`/`f`/`th`). Band-limited (sinc / FFT) resampling is the correct family.  
  Sources: Nyquist–Shannon (standard DSP); torchaudio resample docs (rolloff as fraction of Nyquist, sinc_interp_*): <https://docs.pytorch.org/audio/stable/generated/torchaudio.transforms.Resample.html>; rubato: anti-aliased path is “band-limited interpolation using sinc”: <https://docs.rs/rubato/latest/rubato/>
- rubato guidance: for a **fixed** ratio, `FftFixedIn` / FFT sync resamplers are “fast and high quality”; async sinc matches quality but is heavier; polynomial/async without sinc is faster but lower quality. Cubic sinc + sensible `sinc_len` approaches FFT quality. Defaults lean toward `sinc_len` **256**, oversampling **128**, Cubic, automatic cutoff.  
  Source: <https://docs.rs/rubato/latest/rubato/>
- Our settings (`sinc_len: 64`, Linear, oversampling 64, fixed `f_cutoff: 0.95`) are a **speed-biased** sinc; still anti-aliased, but shorter than rubato’s recommended defaults. Newer rubato prefers `f_cutoff: None` so cutoff is derived via `calculate_cutoff`.  
  Source: rubato `SincInterpolationParameters` / changelog notes in crate docs.

**Native 16 kHz capture vs capture @ 48 kHz then resample**

- Preferring a single high-quality conversion is better than **chained** OS + app resamples (e.g. 48→44.1 then 44.1→16), which our comments already call out as blurring fricatives (`capture.rs`).
- Asking Pulse/PipeWire for “16 kHz” often means **the server** resamples from the hardware rate (commonly 48 kHz). That may be fine or opaque; capturing at the device’s native 48 kHz and doing one rubato/FFT step can be more controllable. Preferring mono @ 16 kHz when *advertised* is still reasonable if it avoids an extra stereo path — but it is not free of OS resampling.
- Avoid 44.1 kHz as an intermediate when 48 kHz is available (already encoded in `pick_input_config`).

**Mapping:** Keep sinc (or switch fixed-ratio path to `FftFixedIn`); do not switch to naive linear/decimate. Optionally raise rubato quality toward defaults or reuse one session-scoped resampler instead of constructing per utterance (`prepare_for_stt`).

---

### 2. Channel downmix

**Primary sources**

- Models expect **monochannel** input (Parakeet card; NeMo results).
- NeMo `channel_selector`: integer channel, subset, or **`"average"`** across channels.  
  Source: <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/api.html>
- onnx-asr Gradio example uses `channel="mean"`.  
  Source: <https://istupakov.github.io/onnx-asr/usage/>

**Desktop-mic reality**

- Many USB “mono” mics appear as **stereo with near-duplicate or L-only energy**. Averaging near-duplicates that are slightly out of phase can **notch high frequencies** (comb filtering) — exactly the band that distinguishes `/f/` vs `/s/`.
- Our **louder-channel** pick (`prepare_for_stt` + `frame_rms` peak-of-channels) is a deliberate departure from NeMo’s average and is appropriate for this capture class. Prefer louder-channel (or first non-silent channel) over mean unless channels are known true stereo content.

---

### 3. Level: peak vs RMS, targets, AGC, clipping

**Evidence in-repo**

- Face fixture peak ~0.18 → empty; peak-norm to 0.7 → `Face.`  
  Sources: `src-tauri/tests/fixtures/stt/face_space/README.md`, `baseline.json`; live path `amplify` TARGET_PEAK = 0.7 (~−3 dBFS).

**Model-side normalization**

- NeMo mel preprocessor commonly uses `normalize: "per_feature"` on the **spectrogram**, not a documented “must peak-normalize waveform to X dBFS” step.  
  Source: NeMo ASR configs — <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/configs.html>
- Waveform level still matters: quiet speech + long silence → weak speech frames relative to zeros; greedy blank path can win (see §4).

**Practice**

| Approach | Pros | Cons |
|----------|------|------|
| Peak normalize (current) | Simple; fixed headroom; fixed face fixture | Dominated by one spike; can leave average speech quieter |
| RMS / LUFS target | Stable loudness for longer speech | Short words + spikes; more tuning |
| Continuous AGC | Handles drifting mic gain | Noise pumping, onset distortion |

- Target peak **0.7 (−3 dBFS)** with **gain cap (~20× / ~26 dB)** and **hard clamp** is a sound short-utterance compromise: recovers quiet mics without unbounded noise boost or hard clipping into mel.
- Avoid streaming AGC that changes mid-utterance; utterance-scoped peak (or RMS) norm is safer.

---

### 4. Silence: trim vs pad; effect on CTC/TDT greedy

**Contrast with Whisper**

- Whisper’s encoder expects a **fixed 30 s** window (`pad_or_trim` to 480_000 samples @ 16 kHz). Short audio is **zero-padded**.  
  Source: <https://github.com/openai/whisper/blob/main/whisper/audio.py>

**Parakeet / NeMo / TDT**

- Variable-length mel → encoder → TDT/CTC-style decode with **blank** for non-emitting steps. Long silence → long blank-only regions; for very quiet speech islands, greedy decode can emit **empty** text (observed on raw `face`).
- NeMo dataset configs often set **`trim_silence: True`** for training data loading — silence is not treated as valuable context by default.  
  Source: <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/configs.html>
- onnx-asr: max audio often **20–30 s** without VAD; longer audio should use VAD segmentation.  
  Source: <https://istupakov.github.io/onnx-asr/usage/>

**Recommendation for short utterances**

- Prefer **trim leading/trailing silence** to a tight speech island, then keep a **small pad** (on the order of **100–300 ms**) so onsets/offsets are not clipped.
- Do **not** rely on multi-second silence pads “for context” with Parakeet the way Whisper pads to 30 s — different architecture.
- Fixture trim variant (±0.2 s around energy island) recovering `Face.` matches this.

---

### 5. VAD: energy vs ML; hangover / pre-roll / post-roll

**Current behavior (problem)**

- Hard gate: `if speech { send frame }` with `VAD_THRESHOLD = 0.005` and **no hangover**. Quiet onsets (unvoiced fricatives) sit below threshold → buffer starts mid-word → garbled `Fez.`-class errors. Comments already note “Missing word beginnings is worse than sending a bit of silence.”

**ML VAD (primary)**

- Silero `get_speech_timestamps`: default `threshold=0.5`, `min_silence_duration_ms=100`, **`speech_pad_ms=30`**, `min_speech_duration_ms=250`.  
  Source: <https://github.com/snakers4/silero-vad> (`utils_vad.py` in repo)
- NeMo Curator VAD segmentation for ASR-oriented segments recommends **`speech_pad_ms=300`** (up to 400–500 for TTS) — padding exists specifically to **preserve onsets/offsets**.  
  Source: <https://docs.nvidia.com/nemo/curator/curate-audio/process-data/quality-filtering/vad>
- onnx-asr integrates Silero/PyAnnote VAD for long-form; docs say **tune VAD parameters**.  
  Source: <https://istupakov.github.io/onnx-asr/usage/#vad-voice-activity-detection>

**Energy VAD (acceptable short-term)**

- Keep a low threshold if desired, but implement:
  - **Pre-roll:** always retain last ~100–300 ms of *rejected* frames when speech starts.
  - **Hangover / post-roll:** continue buffering ~100–300 ms after RMS drops below threshold (or until utterance silence timer).
  - Optional hysteresis (enter threshold higher than exit) to reduce chatter.

Energy VAD remains brittle in noise; Silero-class ML VAD is the better later upgrade, still with pads.

---

### 6. What Parakeet / NeMo / onnx-asr / Whisper say about input format

| Source | Input contract / notes |
|--------|-------------------------|
| [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) | 16 kHz mono wav/flac |
| [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx) | ONNX export for onnx-asr; same NeMo family |
| [NeMo results / transcribe](https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/results.html) | User must mono + 16 kHz before array inference |
| [NeMo configs](https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/configs.html) | `sample_rate: 16000`; mel `AudioToMelSpectrogramPreprocessor`; often `trim_silence: True` |
| [onnx-asr](https://github.com/istupakov/onnx-asr) | Log-mel preprocessors + greedy decode; built-in resample; optional VAD |
| [parakeet-rs](https://docs.rs/parakeet-rs/latest/parakeet_rs/) | 16 kHz mono; stereo auto-downmix |
| [Whisper audio.py](https://github.com/openai/whisper/blob/main/whisper/audio.py) | 16 kHz; **fixed 30 s pad/trim** — not Parakeet’s contract |
| This repo | Mel parity vs onnx-asr `nemo128.onnx` ±1e-4 (`stt.rs` test); contract `STT_SAMPLE_RATE = 16000` |

---

### 7. Phrase boosting / LM vs conversion

**Orthogonal.** NeMo documents GPU-accelerated **n-gram LM fusion** during CTC/RNNT/TDT decoding as a separate customization path from audio I/O.  
Source: <https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/intro.html>

Boosting can reduce short-word confusions (`face`/`space`) *after* the acoustic front-end is sane. It will **not** fix empty decode from silence+level, or onset chopping from hard VAD. Spec already deferred boosting for cutover (`.scratch/host-stt-ort-cutover/spec.md`).

---

## Recommended changes (P0 / P1 / P2)

Mapped to this tree. **P0 items below were implemented in the same iteration as this note** (trim, VAD pad, clean_transcript); P1/P2 remain open.

### P0 — minimal change, highest impact

| Change | Status | Files |
|--------|--------|-------|
| **VAD pre-roll + hangover** (~200 ms) | **Done** (`VadGate`) | `capture.rs` |
| **Energy trim + small pad** before amplify | **Done** (`trim_silence`, ±200 ms) | `mod.rs` |
| **Keep peak-norm 0.7 + gain cap + louder-channel mono** | Keep (no change) | `mod.rs` |
| **Revisit `clean_transcript` `len < 4`** | **Done** (≥2 alphabetic chars) | `mod.rs` |

### P1 — better quality / hygiene

| Change | Why | Files |
|--------|-----|-------|
| **Session-scoped resampler**; prefer **`FftFixedIn`** for fixed device→16k ratio, or raise sinc toward rubato defaults (`sinc_len` 256, Cubic, auto cutoff) | Fixed-ratio FFT is rubato’s recommended high-quality path; avoid per-utterance init | `mod.rs` `prepare_for_stt`; wire rate from `AudioCapture` |
| **Capture policy tune**: if OS 16 kHz is server-resampled junk, prefer **native 48 kHz mono/stereo** + one app resample; keep avoiding 44.1 intermediates | One controlled conversion | `capture.rs` `pick_input_config` — A/B on Host mic |
| **Don’t silently STT at wrong rate** if rubato fails | Contract violation | `run_stt_loop` — skip/emit error instead of `out_rate = sample_rate` when ≠ 16 kHz |

### P2 — later / larger

| Change | Why | Files |
|--------|-----|-------|
| **Silero (or onnx-asr-style) ML VAD** with hysteresis + `speech_pad_ms` ≥ ~100–300 | Better than RMS in noise; matches onnx-asr long-form path | New helper under `audio/`; optional artifact |
| **RMS/LUFS normalize** as alternative to peak for long sentences | More stable loudness | `amplify` |
| **Decoder phrase boost / n-gram LM** | Orthogonal quality for short-word confusions | Outside conversion; engine/decode layer |

---

## Open questions

1. On Host PipeWire/Pulse, does “mono @ 16 kHz” from cpal use a high-quality server resampler, or is **48 kHz capture + rubato/FFT** measurably better on `face`/`space` fricatives?
2. Optimal **pre-roll / hangover** ms for energy VAD at `VAD_THRESHOLD=0.005` on typical desktop noise — 100 vs 300 ms trade false triggers vs onset safety.
3. Does `parakeet-rs` internal stereo downmix (if we ever skip ours) use mean or first channel? Prefer keeping our louder-channel path either way.  
   Source for “converted automatically”: <https://docs.rs/parakeet-rs/latest/parakeet_rs/>
4. How much empty-transcript risk remains after trim+norm with **greedy TDT** vs beam — may need a small fixture matrix beyond face/space.
5. Should utterance end use **endpointing** (VAD silence 300–500 ms) instead of fixed 1 s `SILENCE_MS` for snappier short commands?

---

## Local code / fixture index

- `src-tauri/src/audio/mod.rs` — `amplify`, `prepare_for_stt`, `clean_transcript`, `run_stt_loop`
- `src-tauri/src/audio/capture.rs` — `VAD_THRESHOLD`, `frame_rms`, `pick_input_config`
- `src-tauri/src/audio/stt.rs` — `SttClient`, mel parity, `face_space_diag`
- `src-tauri/src/constants.rs` — `STT_SAMPLE_RATE = 16000`
- `src-tauri/tests/fixtures/stt/face_space/` — wav + f32 + `baseline.json` + README

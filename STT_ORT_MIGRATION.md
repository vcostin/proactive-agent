# STT Migration: Parakeet HTTP → ort in-process

**Goal:** Replace the PyInstaller-frozen Parakeet server with direct ONNX inference
inside the Rust process using the `ort` crate. Same model, same accuracy, no Python.

**Hardware constraint:** CPU only. GPU is reserved exclusively for the LLM.

---

## Why

| Problem with current approach | Solved by ort |
|-------------------------------|---------------|
| 48 MB frozen Python binary per OS | ONNX model file works on all platforms |
| PyInstaller rebuild needed per release | No build step — model file is data |
| Port 5092 hardcoded, can't reassign | No port — direct function call |
| HTTP overhead per utterance | Zero IPC overhead |
| Python runtime risk (crashes, startup time) | Pure Rust |
| macOS needs separate build | Same `.onnx` loads on Windows/macOS/Linux |

The ONNX model the wizard already downloads (`parakeet-tdt-0.6b-v3.onnx`) is the
exact file `ort` will load. Nothing new to distribute.

---

## What the Python server actually does

The Python binary is a FastAPI wrapper around the `onnx-asr` Python library.
Breaking down what `onnx-asr` does for Parakeet:

```
1. Accept WAV bytes (multipart POST)
2. Decode WAV → f32 PCM at 16kHz mono        ← already done by rubato in Rust
3. Compute log-mel spectrogram                 ← ~50 lines of DSP to implement
4. Run ort/ONNX inference (CPU)               ← direct ort::Session call
5. Greedy CTC decode using tokens.txt         ← ~20 lines, tokens file already downloaded
6. Return {"text": "..."}                     ← return String from function
```

Steps 1-2 are already handled by the Rust audio pipeline (VAD + rubato).
Only steps 3-5 are new code.

---

## Implementation plan

### Step 1 — Add dependencies

```toml
# Cargo.toml
ort = { version = "2", default-features = false, features = ["load-dynamic"] }
# rustfft is already transitively available; add explicitly if needed
rustfft = "6"
```

`load-dynamic` means ort loads `onnxruntime.dll` at runtime rather than linking
statically — smaller binary, and we can ship the .dll separately if needed.

### Step 2 — Implement `audio/stt.rs` — new `OrtSttClient`

Replace the HTTP-based `SttClient` with one that holds an `ort::Session`:

```rust
pub struct SttClient {
    session: ort::Session,
    vocab: Vec<String>,   // loaded from parakeet-tdt-0.6b-v3-tokens.txt
}

impl SttClient {
    pub fn new(model_path: &Path, tokens_path: &Path) -> Result<Self> {
        let session = ort::Session::builder()?
            .with_execution_providers([CpuExecutionProvider::default().build()])?
            .with_inter_threads(2)?
            .commit_from_file(model_path)?;

        let vocab = std::fs::read_to_string(tokens_path)?
            .lines()
            .map(str::to_owned)
            .collect();

        Ok(Self { session, vocab })
    }

    pub fn transcribe(&self, pcm_16khz_mono: &[f32]) -> Result<String> {
        let mel = log_mel_spectrogram(pcm_16khz_mono);
        let tensor = ort::inputs!["audio_signal" => mel]?;
        let outputs = self.session.run(tensor)?;
        let token_ids = decode_ctc(&outputs, &self.vocab);
        Ok(token_ids)
    }
}
```

### Step 3 — Log-mel spectrogram

Parakeet TDT expects the standard NeMo log-mel spectrogram:

```
n_fft:       512
hop_length:  160   (10ms at 16kHz)
win_length:  400   (25ms at 16kHz)
n_mels:      80
fmin:        0.0
fmax:        8000.0
```

Implementation using `rustfft`:

```rust
fn log_mel_spectrogram(pcm: &[f32]) -> ndarray::Array2<f32> {
    // 1. Frame audio into overlapping windows (hop_length=160, win_length=400)
    // 2. Apply Hann window
    // 3. FFT each frame (n_fft=512)
    // 4. Compute power spectrum |FFT|²
    // 5. Apply mel filterbank (80 filters, 0–8kHz)
    // 6. Log10 + clamp (log mel)
    // → shape: [1, 80, time_frames]
}
```

**Verification step:** Run the same audio through the Python server and compare
the mel spectrogram tensors numerically. If they match within 1e-4, the
preprocessing is correct. This is the main testable unknown.

### Step 4 — CTC decoding

Greedy CTC: take the argmax of each output frame, collapse repeats, remove blank token.

```rust
fn decode_ctc(outputs: &ort::SessionOutputs, vocab: &[String]) -> String {
    let logits = outputs["log_probs"].extract_tensor::<f32>()?;
    let mut prev = usize::MAX;
    let blank = vocab.len() - 1; // last token is typically <blank>
    logits
        .axis_iter(ndarray::Axis(1))  // time axis
        .filter_map(|frame| {
            let tok = frame.iter().enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?.0;
            if tok != blank && tok != prev { prev = tok; Some(&vocab[tok]) }
            else { prev = tok; None }
        })
        .cloned()
        .collect::<Vec<_>>()
        .join("")
}
```

### Step 5 — Wire into run_stt_loop

`run_stt_loop` currently calls `stt.transcribe(&boosted, rate, channels).await`.
The ort version is synchronous (CPU inference) — wrap in `spawn_blocking`:

```rust
let client = stt_client.clone();
let audio = boosted.clone();
let text = tokio::task::spawn_blocking(move || client.transcribe(&audio))
    .await??;
```

### Step 6 — Initialisation in lib.rs

Create the `SttClient` once at startup and store in app state:

```rust
// In setup hook, after wizard confirms model is ready:
let stt_client = SttClient::new(
    &AppConfig::stt_model_dir().join(STT_MODEL_FILE),
    &AppConfig::stt_model_dir().join(STT_TOKENS_FILE),
)?;
app.manage(Arc::new(stt_client));
```

---

## Verification checklist

- [ ] `cargo check` passes, zero references to `parakeet-server` in source
- [ ] `SttClient::new()` loads model without panic on app startup
- [ ] Mel spectrogram output matches Python server output within 1e-4 (unit test)
- [ ] English 5s utterance → accurate transcript in < 2s on modern CPU
- [ ] Non-native accent utterance → transcript quality same as current Parakeet server
- [ ] No GPU memory consumed during STT (verify with task manager)
- [ ] SidecarHealth debug panel: parakeet row removed
- [ ] Port 5092 no longer appears in monitor loop
- [ ] `parakeet-server-*.exe` removed from `tauri.conf.json` externalBin
- [ ] Wizard no longer shows a binary placeholder row for parakeet

---

## Files that change

| File | Change |
|------|--------|
| `src-tauri/src/audio/stt.rs` | Full rewrite — HTTP client → ort session |
| `src-tauri/src/audio/mod.rs` | `run_stt_loop` uses spawn_blocking for inference |
| `src-tauri/src/lib.rs` | Create SttClient in setup, manage as app state |
| `src-tauri/src/monitor.rs` | Remove parakeet from health check loop |
| `src-tauri/src/commands.rs` | Remove `start_voice_input` parakeet port dependency |
| `src-tauri/Cargo.toml` | Add `ort`, `rustfft` |
| `src-tauri/tauri.conf.json` | Remove parakeet from `externalBin` |
| `src/components/debug/SidecarHealth.tsx` | Remove parakeet row |

---

## What does NOT change

- `audio/capture.rs` — VAD, energy reporting, cpal stream
- `audio/mod.rs` — rubato resampling, amplify, clean_transcript, VAD loop structure
- `audio/tts.rs` — Piper subprocess, unchanged
- All Tauri commands except voice-input plumbing
- The wizard model download (`.onnx` and tokens files already downloaded)
- Frontend voice toggle, waveform, transcript routing

---

## Near-term follow-on: GPU layer offload for LLM

Once Parakeet is out of the GPU picture entirely, the `-ngl` parameter for
llama-server becomes the main VRAM control knob. A slider in the Models tab
(`GPU layers: 0 ← ——— → max`) would let users trade VRAM for speed:

- `ngl=0` → full CPU inference, 0 VRAM used
- `ngl=N` → first N transformer layers on GPU, rest on CPU
- `ngl=999` → all layers on GPU (current default)

LM Studio calls this "GPU Offload". llama-server already supports `-ngl`.
It's a one-liner config change + one UI slider. No model changes needed.

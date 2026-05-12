/// Speech-to-text via Parakeet TDT ONNX model — in-process via ort.
///
/// Replaces the PyInstaller frozen parakeet-server.exe with direct ONNX
/// Runtime inference. Same model, same accuracy, no Python, no HTTP, no port.
///
/// Execution provider: CPU only — GPU is reserved exclusively for the LLM.
///
/// Pipeline:
///   16kHz mono PCM  (rubato resampled in audio/mod.rs)
///   → log-mel spectrogram  (NeMo params: 80 mels, 25ms/10ms window/hop)
///   → ort CPU inference    (Parakeet TDT ONNX)
///   → greedy CTC decode    (tokens.txt vocabulary)
///   → transcript String

use anyhow::{bail, Context, Result};
use ndarray::{Array1, Array2, Array3, Axis};
use ort::{inputs, session::Session, value::Tensor};
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ── NeMo Parakeet TDT spectrogram parameters ─────────────────────────────────
// Source: NVIDIA NeMo Parakeet TDT model card + onnx-asr preprocessing code.
// These must match exactly — wrong parameters → correct code, wrong transcripts.
const N_FFT:      usize = 512;
const HOP_LENGTH: usize = 160;   // 10 ms at 16 kHz
const WIN_LENGTH: usize = 400;   // 25 ms at 16 kHz
const N_MELS:     usize = 80;
const FMIN:       f32   = 0.0;
const FMAX:       f32   = 8000.0;
const MEL_FLOOR:  f32   = 1e-5;  // clips log(0) → log(MEL_FLOOR)

// ── Client ────────────────────────────────────────────────────────────────────

pub struct SttClient {
    // Mutex needed: Session::run() takes &mut self, but SttClient is shared as Arc
    session: std::sync::Mutex<Session>,
    vocab: Vec<String>,
    blank_id: usize,
    last_latency_ms: Arc<AtomicU64>,
}

impl SttClient {
    /// Load the ONNX model and vocabulary. Call once at app startup.
    /// Model path: binaries/parakeet/models/parakeet-tdt-0.6b-v3.onnx
    /// Vocab path: binaries/parakeet/models/parakeet-tdt-0.6b-v3-tokens.txt
    pub fn new(model_path: &Path, tokens_path: &Path) -> Result<Self> {
        use ort::execution_providers::CPUExecutionProvider;

        // Load onnxruntime.dll from piper's directory — we already distribute it
        // with Piper, so no extra file to ship. ort needs the path at init time
        // when using the load-dynamic feature.
        let ort_dylib = crate::binaries_dir()
            .join("piper")
            .join("onnxruntime.dll");

        eprintln!("[STT] ort dylib path: {:?} (exists: {})", ort_dylib, ort_dylib.exists());
        eprintln!("[STT] model path:     {:?} (exists: {})", model_path, model_path.exists());
        eprintln!("[STT] tokens path:    {:?} (exists: {})", tokens_path, tokens_path.exists());

        if ort_dylib.exists() {
            eprintln!("[STT] calling ort::init_from...");
            ort::init_from(&ort_dylib)
                .map_err(|e| anyhow::anyhow!("ort init_from({:?}): {e}", ort_dylib))?
                .commit();
            eprintln!("[STT] ort::init_from succeeded");
        } else {
            eprintln!("[STT] ort dylib NOT FOUND — falling back to PATH search");
            ort::init().commit();
        }

        eprintln!("[STT] calling Session::builder...");
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("ort builder: {e}"))?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .map_err(|e| anyhow::anyhow!("ort providers: {e}"))?
            .with_inter_threads(2)
            .map_err(|e| anyhow::anyhow!("ort threads: {e}"))?
            .commit_from_file(model_path)
            .context("failed to load Parakeet ONNX model")?;

        eprintln!("[STT] Session loaded successfully");

        let vocab: Vec<String> = std::fs::read_to_string(tokens_path)
            .context("failed to load tokens.txt vocabulary")?
            .lines()
            .map(str::to_owned)
            .collect();

        let blank_id = vocab.len().saturating_sub(1); // last token is <blank> in CTC

        eprintln!("[STT] vocab size: {}, blank_id: {}", vocab.len(), blank_id);

        Ok(Self {
            session: std::sync::Mutex::new(session),
            vocab,
            blank_id,
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Transcribe 16 kHz mono PCM. **Synchronous** — call from `spawn_blocking`.
    pub fn transcribe(&self, pcm_16khz_mono: &[f32]) -> Result<String> {
        if pcm_16khz_mono.is_empty() { return Ok(String::new()); }
        let start = std::time::Instant::now();

        // Step 1 — log-mel spectrogram → [N_MELS, time_frames]
        let mel = log_mel_spectrogram(pcm_16khz_mono)?;
        let time_frames = mel.ncols() as i64;

        // Step 2 — add batch dim → [1, N_MELS, time_frames]
        let mel_3d: Array3<f32> = mel.insert_axis(Axis(0));
        let length  = Array1::<i64>::from_vec(vec![time_frames]);

        // Step 3 — run ONNX inference (lock Mutex for &mut Session)
        let mel_tensor = Tensor::<f32>::from_array(mel_3d)
            .map_err(|e| anyhow::anyhow!("mel tensor: {e}"))?;
        let len_tensor = Tensor::<i64>::from_array(length)
            .map_err(|e| anyhow::anyhow!("len tensor: {e}"))?;

        let session_inputs = inputs![
            "audio_signal" => mel_tensor,
            "length"       => len_tensor,
        ];

        let mut guard = self.session
            .lock()
            .map_err(|_| anyhow::anyhow!("STT session mutex poisoned"))?;
        let outputs = guard
            .run(session_inputs)
            .map_err(|e| anyhow::anyhow!("ort run: {e}"))?;

        // Step 4 — greedy CTC decode
        // Expected output shape: [batch=1, time, vocab_size]
        let log_probs_dyn = outputs["log_probs"]
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("extract output: {e}"))?;

        let log_probs = log_probs_dyn
            .into_dimensionality::<ndarray::Ix3>()
            .context("expected log_probs shape [batch, time, vocab]")?;

        let transcript = greedy_ctc(log_probs.view(), &self.vocab, self.blank_id);

        let elapsed = start.elapsed().as_millis() as u64;
        self.last_latency_ms.store(elapsed, Ordering::Relaxed);

        Ok(transcript)
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

// ── Log-mel spectrogram ───────────────────────────────────────────────────────

fn log_mel_spectrogram(pcm: &[f32]) -> Result<Array2<f32>> {
    let mel_filters = mel_filterbank();

    // Hann window
    let hann: Vec<f32> = (0..WIN_LENGTH)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / WIN_LENGTH as f32).cos()))
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);

    // Centre-pad the signal by N_FFT/2 on each side (NeMo behaviour)
    let pad = N_FFT / 2;
    let mut padded = vec![0.0f32; pad + pcm.len() + pad];
    padded[pad..pad + pcm.len()].copy_from_slice(pcm);

    let n_frames = padded.len().saturating_sub(WIN_LENGTH) / HOP_LENGTH + 1;
    if n_frames == 0 { bail!("audio clip too short for mel spectrogram"); }

    let mut mel_spec = Array2::<f32>::zeros((N_MELS, n_frames));

    for frame in 0..n_frames {
        let start = frame * HOP_LENGTH;
        let mut buf: Vec<Complex<f32>> = (0..N_FFT)
            .map(|i| {
                let s = if i < WIN_LENGTH { padded[start + i] * hann[i] } else { 0.0 };
                Complex::new(s, 0.0)
            })
            .collect();
        fft.process(&mut buf);

        // One-sided power spectrum
        let power: Vec<f32> = buf[..N_FFT / 2 + 1].iter().map(|c| c.norm_sqr()).collect();

        // Apply mel filterbank → log
        for (m, filter) in mel_filters.iter().enumerate() {
            let energy: f32 = filter.iter().zip(&power).map(|(w, p)| w * p).sum();
            mel_spec[[m, frame]] = energy.max(MEL_FLOOR).ln();
        }
    }

    // Per-feature (per-mel-channel) normalisation — NeMo PerFeatureNormalizer
    for mut row in mel_spec.rows_mut() {
        let n = row.len() as f32;
        let mean = row.sum() / n;
        let var  = row.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n;
        let std  = var.sqrt().max(1e-5);
        row.mapv_inplace(|v| (v - mean) / std);
    }

    Ok(mel_spec)
}

/// Mel filterbank matrix — [N_MELS, N_FFT/2+1].
/// Computed once per call (cheap — N_MELS=80 filters, N_FFT/2+1=257 bins).
fn mel_filterbank() -> Vec<Vec<f32>> {
    let hz_to_mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
    let mel_to_hz = |m: f32| 700.0 * (10.0f32.powf(m / 2595.0) - 1.0);

    let mel_min = hz_to_mel(FMIN);
    let mel_max = hz_to_mel(FMAX);
    // N_MELS + 2 evenly-spaced mel points (includes lower and upper edges)
    let pts: Vec<f32> = (0..=N_MELS + 1)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f32 / (N_MELS + 1) as f32))
        .collect();

    let fft_freqs: Vec<f32> = (0..=N_FFT / 2)
        .map(|i| i as f32 * crate::constants::STT_SAMPLE_RATE as f32 / N_FFT as f32)
        .collect();

    (0..N_MELS).map(|m| {
        fft_freqs.iter().map(|&f| {
            let (lo, center, hi) = (pts[m], pts[m + 1], pts[m + 2]);
            if      f >= lo     && f <= center { (f - lo)     / (center - lo) }
            else if f >  center && f <= hi     { (hi - f)     / (hi - center) }
            else                               { 0.0 }
        }).collect()
    }).collect()
}

// ── Greedy CTC decode ─────────────────────────────────────────────────────────

fn greedy_ctc(
    log_probs: ndarray::ArrayView3<f32>,
    vocab: &[String],
    blank_id: usize,
) -> String {
    // Shape: [batch=1, time, vocab_size]
    let mut prev = blank_id;
    log_probs
        .index_axis(Axis(0), 0) // single batch
        .rows()
        .into_iter()
        .filter_map(|frame| {
            let tok = frame
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(blank_id);
            let emit = tok != blank_id && tok != prev;
            prev = tok;
            if emit { vocab.get(tok).cloned() } else { None }
        })
        .collect::<Vec<_>>()
        .join("")
}

// ── pcm_to_wav kept for compatibility (used in tests) ────────────────────────

pub fn pcm_to_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        let i = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        wav.extend_from_slice(&i.to_le_bytes());
    }
    wav
}

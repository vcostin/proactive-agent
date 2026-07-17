//! In-process Host STT path via ONNX Runtime (`ort`) + Parakeet TDT.
//!
//! Loads the app-managed ORT shared library with `load-dynamic`, then runs
//! greedy TDT decode on CPU only. Soft-fail: callers get `Err` with rich
//! diagnostics when artifacts or engine init fail.

use anyhow::{anyhow, bail, Context, Result};
use parakeet_rs::{ExecutionConfig, ExecutionProvider, ParakeetTDT, Transcriber};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Instant;

static ORT_INIT: Once = Once::new();
static ORT_INIT_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn init_ort_once(ort_lib: &Path) -> Result<()> {
    let path = ort_lib.to_path_buf();
    ORT_INIT.call_once(|| {
        if let Err(e) = ort::init_from(path.to_string_lossy().as_ref()) {
            *ORT_INIT_ERROR.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("failed to load ONNX Runtime from {}: {e:#}", path.display()));
        }
    });
    if let Some(err) = ORT_INIT_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        bail!("{err} (Host STT soft-fail)");
    }
    Ok(())
}

/// In-process STT engine (Host STT path after ort cutover).
pub struct SttClient {
    engine: Mutex<ParakeetTDT>,
    last_latency_ms: Arc<AtomicU64>,
}

impl SttClient {
    /// Load ORT from `ort_lib_dir`, then Parakeet TDT from `model_dir`.
    ///
    /// `model_dir` must contain encoder + decoder_joint ONNX and `vocab.txt`
    /// (see Platform catalog `stt-encoder` / `stt-decoder` / `stt-vocab`).
    pub fn new(model_dir: &Path, ort_lib_dir: &Path) -> Result<Self> {
        let ort_lib = resolve_ort_dylib(ort_lib_dir).with_context(|| {
            format!(
                "ONNX Runtime library missing under {} — open Setup Wizard / Setup repair",
                ort_lib_dir.display()
            )
        })?;

        // load-dynamic: missing ORT must soft-fail at init, not prevent process start.
        init_ort_once(&ort_lib)?;

        let exec = ExecutionConfig::new()
            .with_execution_provider(ExecutionProvider::Cpu)
            .with_intra_threads(2)
            .with_inter_threads(1);

        let engine = ParakeetTDT::from_pretrained(model_dir, Some(exec)).map_err(|e| {
            anyhow!(
                "Parakeet TDT engine load failed from {}: {e:#}\n\
                 model_dir entries: {}\n\
                 ort_lib: {}\n\
                 Open Setup Wizard / Setup repair to restore Host STT artifacts.",
                model_dir.display(),
                list_dir_debug(model_dir),
                ort_lib.display()
            )
        })?;

        Ok(Self {
            engine: Mutex::new(engine),
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Transcribe mono (or multi-channel) PCM. Prefer STT input contract: mono @ 16 kHz.
    pub fn transcribe(&self, pcm: &[f32], sample_rate: u32, channels: u16) -> Result<String> {
        let start = Instant::now();
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| anyhow!("STT engine lock poisoned"))?;
        let result = engine
            .transcribe_samples(pcm.to_vec(), sample_rate, channels, None)
            .map_err(|e| anyhow!("STT inference failed: {e:#}"))?;
        self.last_latency_ms
            .store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(result.text.trim().to_string())
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

/// Resolve the app-managed ORT dylib under `ort_lib_dir`.
pub fn resolve_ort_dylib(ort_lib_dir: &Path) -> Result<PathBuf> {
    if !ort_lib_dir.is_dir() {
        bail!("{} is not a directory", ort_lib_dir.display());
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(ort_lib_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() && !path.is_symlink() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_lowercase();
        if lower == "libonnxruntime.so"
            || lower.starts_with("libonnxruntime.so.")
            || lower == "libonnxruntime.dylib"
            || lower == "onnxruntime.dll"
        {
            candidates.push(path);
        }
    }
    // Prefer unversioned / primary soname names.
    candidates.sort_by_key(|p| {
        let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("").len();
        n
    });
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no onnxruntime shared library in {}", ort_lib_dir.display()))
}

fn list_dir_debug(dir: &Path) -> String {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(", "),
        Err(e) => format!("<unreadable: {e}>"),
    }
}

/// Encode f32 PCM as 16-bit WAV (kept for fixtures / debug capture).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::STT_SAMPLE_RATE;
    use ndarray::Array3;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stt")
    }

    fn load_f32_pcm(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("read pcm fixture");
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn load_mel_ref(path: &Path, shape: [usize; 3]) -> Array3<f32> {
        let bytes = std::fs::read(path).expect("read mel fixture");
        let data: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Array3::from_shape_vec(shape, data).expect("mel shape")
    }

    /// Mel parity vs onnx-asr's nemo128.onnx preprocessor (same file onnx_asr ships).
    /// We run that ONNX through ort — matching Python/`onnx-asr` within ±1e-4.
    #[test]
    fn mel_nemo128_ort_matches_checked_in_reference_within_1e4() {
        let dir = fixtures_dir();
        let ort_candidates = [
            PathBuf::from("binaries/ort"),
            PathBuf::from("../binaries/ort"),
            PathBuf::from(".cache/parakeet-tdt/.venv/lib/python3.12/site-packages/onnxruntime/capi"),
            PathBuf::from("../.cache/parakeet-tdt/.venv/lib/python3.12/site-packages/onnxruntime/capi"),
        ];
        let ort_dir = ort_candidates.into_iter().find(|p| {
            let root = if p.is_absolute() {
                p.clone()
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(p)
            };
            resolve_ort_dylib(&root).is_ok()
        });
        let Some(rel) = ort_dir else {
            eprintln!("skip mel parity: no ORT dylib found for test");
            return;
        };
        let ort_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(&rel);
        let ort_root = std::fs::canonicalize(&ort_root).unwrap_or(ort_root);
        let dylib = resolve_ort_dylib(&ort_root).expect("ort dylib");
        init_ort_once(&dylib).expect("init ort");

        let pcm = load_f32_pcm(&dir.join("fixture_440hz_1s.f32"));
        let mel_path = dir.join("nemo128.onnx");
        assert!(mel_path.is_file(), "nemo128.onnx fixture missing");

        let mut session = ort::session::Session::builder()
            .expect("session builder")
            .commit_from_file(&mel_path)
            .expect("load nemo128");

        let waveforms = ndarray::Array2::from_shape_vec((1, pcm.len()), pcm).unwrap();
        let waveforms_lens = ndarray::Array1::from_vec(vec![waveforms.ncols() as i64]);
        let wf = ort::value::Value::from_array(waveforms).expect("waveforms value");
        let lens = ort::value::Value::from_array(waveforms_lens).expect("lens value");
        let outputs = session
            .run(ort::inputs![
                "waveforms" => wf,
                "waveforms_lens" => lens,
            ])
            .expect("nemo128 run");
        let features = outputs["features"]
            .try_extract_tensor::<f32>()
            .expect("features tensor");
        let (shape, data) = features;
        let shape_usize: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        assert_eq!(shape_usize, vec![1, 128, 101]);

        let reference = load_mel_ref(
            &dir.join("fixture_440hz_1s_mel_nemo128.f32"),
            [1, 128, 101],
        );
        let mut max_abs = 0.0f32;
        for (i, &v) in data.iter().enumerate() {
            let r = reference.as_slice().unwrap()[i];
            max_abs = max_abs.max((v - r).abs());
        }
        assert!(
            max_abs <= 1e-4,
            "mel max abs diff {max_abs} exceeds ±1e-4 vs onnx-asr nemo128 reference"
        );
        let _ = STT_SAMPLE_RATE;
    }

    /// Diagnostic: face vs space greedy transcripts (+ gain / trim variants).
    /// Run: `cargo test -p proactive-agent-lib face_space_diag -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn face_space_diag() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let model_dir = root.join("../binaries/parakeet/models");
        let ort_dir = root.join("../binaries/ort");
        let client = SttClient::new(&model_dir, &ort_dir).expect("load Host STT engine");
        let dir = fixtures_dir().join("face_space");

        for id in ["face", "space"] {
            let pcm = load_f32_pcm(&dir.join(format!("{id}_16k.f32")));
            let peak = pcm.iter().copied().map(f32::abs).fold(0.0f32, f32::max);
            let gain = if peak > 1e-6 { 0.9 / peak } else { 1.0 };
            let boosted: Vec<f32> = pcm.iter().map(|x| (x * gain).clamp(-1.0, 1.0)).collect();
            // Match live voice path TARGET_PEAK = 0.7
            let live_gain = if peak > 0.001 {
                (0.7 / peak).min(20.0)
            } else {
                1.0
            };
            let live_amp: Vec<f32> = pcm
                .iter()
                .map(|x| (x * live_gain).clamp(-1.0, 1.0))
                .collect();

            // Trim to first energy island (±0.2s pad)
            let win = 320; // 20ms @ 16k
            let energies: Vec<f32> = pcm
                .chunks(win)
                .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
                .collect();
            let thr = energies.iter().copied().fold(0.0f32, f32::max) * 0.15;
            let active: Vec<usize> = energies
                .iter()
                .enumerate()
                .filter(|(_, e)| **e >= thr)
                .map(|(i, _)| i)
                .collect();
            let trimmed = if let (Some(&a), Some(&b)) = (active.first(), active.last()) {
                let start = a.saturating_sub(10) * win;
                let end = ((b + 11) * win).min(pcm.len());
                pcm[start..end].to_vec()
            } else {
                pcm.clone()
            };

            for (label, audio) in [
                ("raw", &pcm),
                ("live0.7", &live_amp),
                ("peak0.9", &boosted),
                ("trim", &trimmed),
            ] {
                let text = client
                    .transcribe(audio, STT_SAMPLE_RATE, 1)
                    .unwrap_or_else(|e| format!("ERR:{e:#}"));
                eprintln!(
                    "{id}/{label}: {:>6.2}s peak={:.3} -> {:?}",
                    audio.len() as f32 / STT_SAMPLE_RATE as f32,
                    audio.iter().copied().map(f32::abs).fold(0.0f32, f32::max),
                    text
                );
            }
        }
    }

    #[test]
    fn fixture_hello_world_transcript_parity() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let model_dir = root.join("../binaries/parakeet/models");
        let ort_dir = root.join("../binaries/ort");
        if resolve_ort_dylib(&ort_dir).is_err()
            || !model_dir.join(crate::constants::STT_ENCODER_FILE).is_file()
            || !model_dir.join(crate::constants::STT_DECODER_FILE).is_file()
            || !model_dir.join(crate::constants::STT_VOCAB_FILE).is_file()
        {
            eprintln!("skip transcript parity: Host STT artifacts not present under binaries/");
            return;
        }

        let client = SttClient::new(&model_dir, &ort_dir).expect("load Host STT engine");
        let pcm = load_f32_pcm(&fixtures_dir().join("hello_world_16k.f32"));
        let text = client
            .transcribe(&pcm, STT_SAMPLE_RATE, 1)
            .expect("transcribe");
        // Agreed normalization: trim + exact match to Piper-generated fixture phrase.
        assert_eq!(
            text.trim(),
            "Hello world.",
            "ort greedy TDT transcript must match fixture (sidecar parity gate)"
        );
    }

    #[test]
    fn resolve_ort_dylib_finds_so_in_fixture_dir() {
        let root = std::env::temp_dir().join(format!(
            "pa-ort-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let lib = root.join("libonnxruntime.so.1.23.2");
        std::fs::write(&lib, [0u8; 8]).unwrap();
        let found = resolve_ort_dylib(&root).unwrap();
        assert_eq!(found, lib);
        let _ = std::fs::remove_dir_all(&root);
    }
}

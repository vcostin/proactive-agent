#![allow(dead_code)]
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// TTS client that calls sherpa-onnx as a subprocess (no HTTP server needed).
/// Falls back to HTTP /v1/audio/speech if the CLI binary isn't found.
pub struct TtsClient {
    last_latency_ms: Arc<AtomicU64>,
}

impl TtsClient {
    pub fn new(_port: u16) -> Self {
        Self { last_latency_ms: Arc::new(AtomicU64::new(0)) }
    }

    /// Synthesise `text` and play through the default output device.
    /// Splits into sentences for lower latency.
    pub async fn speak(&self, text: &str) -> Result<()> {
        for sentence in split_sentences(text) {
            if sentence.trim().is_empty() { continue; }
            let wav = self.synthesise_chunk(sentence.trim()).await?;
            let pcm = wav_to_f32(&wav);
            tokio::task::spawn_blocking(move || play_pcm_blocking(&pcm))
                .await
                .map_err(|e| anyhow::anyhow!("playback panic: {e}"))??;
        }
        Ok(())
    }

    async fn synthesise_chunk(&self, text: &str) -> Result<Vec<u8>> {
        let start = Instant::now();

        // Locate the sherpa-onnx TTS binary
        let binary = crate::find_sidecar("kokoro-server");

        let result = if let Some(bin) = binary {
            self.synthesise_via_cli(&bin, text).await
        } else {
            Err(anyhow::anyhow!("TTS binary not found — run: npm run setup"))
        };

        self.last_latency_ms.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        result
    }

    /// Call sherpa-onnx CLI subprocess, capture WAV output.
    async fn synthesise_via_cli(&self, binary: &PathBuf, text: &str) -> Result<Vec<u8>> {
        let bin_dir = binary.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::binaries_dir);

        // Locate the piper voice model
        let models_dir = {
            let cwd = std::env::current_dir().unwrap_or_default();
            let root = match cwd.file_name().and_then(|n| n.to_str()) {
                Some("src-tauri") => cwd.parent().unwrap_or(&cwd).to_path_buf(),
                _ => cwd,
            };
            root.join("models").join("tts")
        };
        let model_path   = models_dir.join("en_US-lessac-medium.onnx");
        let tokens_path  = models_dir.join("en_US-lessac-medium.onnx.json");

        if !model_path.exists() {
            return Err(anyhow::anyhow!("TTS model not found — run: npm run setup"));
        }

        let tmp_wav = std::env::temp_dir().join("proactive_tts.wav");

        let output = tokio::process::Command::new(binary)
            .args([
                &format!("--vits-model={}", model_path.display()),
                &format!("--vits-tokens={}", tokens_path.display()),
                &format!("--output-filename={}", tmp_wav.display()),
                &format!("--input-text={text}"),
                "--num-threads=2",
                "--sid=0",
            ])
            .current_dir(&bin_dir)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("sherpa-onnx error: {err}"));
        }

        let bytes = tokio::fs::read(&tmp_wav).await?;
        let _ = tokio::fs::remove_file(&tmp_wav).await;
        Ok(bytes)
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n') && !buf.trim().is_empty() {
            sentences.push(buf.trim().to_string());
            buf.clear();
        }
    }
    if !buf.trim().is_empty() { sentences.push(buf.trim().to_string()); }
    sentences
}

fn wav_to_f32(wav: &[u8]) -> Vec<f32> {
    if wav.len() < 44 { return vec![]; }
    wav[44..].chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

fn play_pcm_blocking(samples: &[f32]) -> Result<()> {
    if samples.is_empty() { return Ok(()); }
    let host = cpal::default_host();
    let device = host.default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config: cpal::StreamConfig = device.default_output_config()?.into();
    let samples = Arc::new(samples.to_vec());
    let pos = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    let samples_cb = samples.clone();
    let pos_cb = pos.clone();
    let stream = device.build_output_stream::<f32, _, _>(
        &config,
        move |out: &mut [f32], _| {
            let p = pos_cb.load(Ordering::Relaxed);
            let available = samples_cb.len().saturating_sub(p);
            let copy = available.min(out.len());
            out[..copy].copy_from_slice(&samples_cb[p..p + copy]);
            for s in &mut out[copy..] { *s = 0.0; }
            pos_cb.fetch_add(copy, Ordering::Relaxed);
            if copy == 0 { let _ = tx.try_send(()); }
        },
        |err| eprintln!("[AUDIO] playback error: {err}"),
        None,
    )?;
    stream.play()?;
    let _ = rx.recv();
    Ok(())
}

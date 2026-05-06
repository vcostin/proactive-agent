#![allow(dead_code)]
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Speech-to-text via whisper.cpp CLI subprocess.
/// No HTTP server needed — call the binary directly with a temp WAV file.
pub struct SttClient {
    model_path: PathBuf,
    last_latency_ms: Arc<AtomicU64>,
}

impl SttClient {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Transcribe PCM audio using whisper CLI subprocess.
    pub async fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        channels: u16,
    ) -> Result<String> {
        let start = Instant::now();

        // Find whisper binary (whisper-cli or whisper-server in binaries/whisper/)
        let binary = crate::find_sidecar("whisper-cli")
            .or_else(|| crate::find_sidecar("whisper-server"))
            .ok_or_else(|| anyhow::anyhow!("whisper binary not found — run: npm run setup"))?;

        let bin_dir = binary.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::binaries_dir);

        if !self.model_path.exists() {
            return Err(anyhow::anyhow!(
                "whisper model not found at {} — run: npm run setup",
                self.model_path.display()
            ));
        }

        // Write temp WAV file
        let tmp = std::env::temp_dir().join("proactive_stt.wav");
        let wav = pcm_to_wav(pcm, sample_rate, channels);
        tokio::fs::write(&tmp, &wav).await?;

        // Call whisper CLI
        let output = tokio::process::Command::new(&binary)
            .args([
                "-m", self.model_path.to_str().unwrap_or(""),
                "-f", tmp.to_str().unwrap_or(""),
                "-l", "en",
                "--no-prints",       // suppress progress bars
                "--no-timestamps",   // plain text output only
            ])
            .current_dir(&bin_dir)
            .output()
            .await;

        let _ = tokio::fs::remove_file(&tmp).await;

        let output = output?;
        self.last_latency_ms.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(text)
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("whisper error: {err}"))
        }
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

/// Encode f32 PCM as 16-bit WAV.
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

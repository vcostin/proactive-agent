#![allow(dead_code)]
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct SttClient {
    client: Client,
    base_url: String,
    last_latency_ms: Arc<AtomicU64>,
}

#[derive(Deserialize)]
struct TranscriptResponse {
    text: String,
}

impl SttClient {
    pub fn new(port: u16) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Send accumulated PCM samples to whisper.cpp and return the transcript.
    pub async fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        channels: u16,
    ) -> Result<String> {
        let wav = pcm_to_wav(pcm, sample_rate, channels);
        let start = Instant::now();

        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp: TranscriptResponse = self
            .client
            .post(format!("{}/inference", self.base_url))
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.last_latency_ms
            .store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(resp.text.trim().to_string())
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

/// Encode f32 PCM samples as a 16-bit mono/stereo WAV.
fn pcm_to_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32; // 16-bit = 2 bytes per sample
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);

    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        let i = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        wav.extend_from_slice(&i.to_le_bytes());
    }
    wav
}

#![allow(dead_code)]
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use reqwest::Client;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct TtsClient {
    client: Client,
    base_url: String,
    last_latency_ms: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct TtsRequest<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
}

impl TtsClient {
    pub fn new(port: u16) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            last_latency_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Synthesise `text` in sentence-sized chunks and play each chunk as it
    /// arrives, so playback starts before the full text is synthesised.
    pub async fn speak(&self, text: &str) -> Result<()> {
        for sentence in split_sentences(text) {
            if sentence.trim().is_empty() {
                continue;
            }
            let wav = self.synthesise_chunk(sentence.trim()).await?;
            let pcm = wav_to_f32(&wav);
            // Playback on a blocking thread so it doesn't stall the async runtime
            tokio::task::spawn_blocking(move || play_pcm_blocking(&pcm))
                .await
                .map_err(|e| anyhow::anyhow!("playback task panicked: {e}"))??;
        }
        Ok(())
    }

    async fn synthesise_chunk(&self, text: &str) -> Result<Vec<u8>> {
        let start = Instant::now();
        // Kokoro server — OpenAI TTS-compatible endpoint.
        // EXTEND: adjust endpoint / request shape to match the specific Kokoro build in use.
        let body = TtsRequest { model: "kokoro", input: text, voice: "af_heart" };
        let bytes = self
            .client
            .post(format!("{}/v1/audio/speech", self.base_url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();

        self.last_latency_ms
            .store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(bytes)
    }

    pub fn last_latency_ms(&self) -> u64 {
        self.last_latency_ms.load(Ordering::Relaxed)
    }
}

/// Split text at sentence boundaries for chunked synthesis.
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
    if !buf.trim().is_empty() {
        sentences.push(buf.trim().to_string());
    }
    sentences
}

/// Decode 16-bit PCM WAV bytes to f32 samples.
/// Minimal parser — assumes 44-byte header and 16-bit samples.
fn wav_to_f32(wav: &[u8]) -> Vec<f32> {
    if wav.len() < 44 {
        return vec![];
    }
    wav[44..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

/// Synchronous cpal playback — must be called from a blocking thread.
fn play_pcm_blocking(samples: &[f32]) -> Result<()> {
    if samples.is_empty() {
        return Ok(());
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no audio output device"))?;
    let config: cpal::StreamConfig = device.default_output_config()?.into();

    let samples = Arc::new(samples.to_vec());
    let pos = Arc::new(AtomicUsize::new(0));

    let samples_cb = samples.clone();
    let pos_cb = pos.clone();

    let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(1);

    let stream = device.build_output_stream::<f32, _, _>(
        &config,
        move |out: &mut [f32], _| {
            let p = pos_cb.load(Ordering::Relaxed);
            let available = samples_cb.len().saturating_sub(p);
            let copy = available.min(out.len());
            out[..copy].copy_from_slice(&samples_cb[p..p + copy]);
            // Silence the remainder of the output buffer
            for s in &mut out[copy..] {
                *s = 0.0;
            }
            pos_cb.fetch_add(copy, Ordering::Relaxed);
            if copy == 0 {
                // All samples drained — signal completion
                let _ = done_tx.try_send(());
            }
        },
        |err| eprintln!("[AUDIO] playback error: {err}"),
        None,
    )?;

    stream.play()?;
    // Block until playback finishes or device errors
    let _ = done_rx.recv();
    Ok(())
}

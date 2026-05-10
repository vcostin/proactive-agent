#![allow(dead_code, unused_imports)]
pub mod capture;
pub mod stt;
pub mod tts;

pub use capture::AudioCapture;
pub use stt::SttClient;
pub use tts::TtsClient;

use anyhow::Result;
use tauri::Emitter;
use tokio::sync::mpsc;

const AUDIO_CHANNEL_BUF: usize = 256;
const SILENCE_MS: u64 = 1000; // 1s silence before sending — captures complete sentences
const MIC_GAIN: f32 = 1.5;   // kept for API compat, replaced by normalization in amplify()

/// Filter out common STT hallucinations for non-speech audio.
/// Returns empty string if the transcript should be discarded.
/// Normalize audio to a target peak level regardless of mic volume.
/// Better than fixed gain — adapts to the actual signal level automatically.
fn amplify(samples: &[f32], _gain: f32) -> Vec<f32> {
    const TARGET_PEAK: f32 = 0.7; // -3 dBFS — good level for ASR models
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak < 0.001 { return samples.to_vec(); } // silence — nothing to boost
    let gain = (TARGET_PEAK / peak).min(20.0); // cap at 26dB to avoid noise amplification
    samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Downmix stereo→mono then resample to STT_SAMPLE_RATE (16 kHz) using
/// rubato's FFT-based sinc resampler — high quality, handles any device rate.
/// The ratio is computed from the actual cpal device rate, so this works
/// correctly for 44100, 48000, 96000 Hz or anything else a mic might report.
fn prepare_for_stt(samples: &[f32], device_rate: u32, channels: u16) -> Vec<f32> {
    use rubato::{FftFixedIn, Resampler};
    use crate::constants::STT_SAMPLE_RATE;

    // Step 1: downmix to mono
    let mono: Vec<f32> = if channels <= 1 {
        samples.to_vec()
    } else {
        let ch = channels as usize;
        samples.chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    // Step 2: resample to 16 kHz (no-op if device already runs at that rate)
    if device_rate == STT_SAMPLE_RATE || mono.is_empty() {
        return mono;
    }

    let chunk_size = mono.len();
    let resampler = FftFixedIn::<f32>::new(
        device_rate as usize,
        STT_SAMPLE_RATE as usize,
        chunk_size,
        2,   // sub-chunks — 2 balances quality vs memory
        1,   // channels (mono at this point)
    );

    match resampler {
        Ok(mut r) => {
            match r.process(&[mono.clone()], None) {
                Ok(out) => out.into_iter().next().unwrap_or_default(),
                Err(e) => {
                    eprintln!("[STT] rubato resample error: {e} — falling back to raw audio");
                    mono
                }
            }
        }
        Err(e) => {
            eprintln!("[STT] rubato init error: {e} — falling back to raw audio");
            mono
        }
    }
}

fn clean_transcript(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() { return String::new(); }

    // Whisper hallucinates these for silence, music, background noise
    let hallucinations = [
        "[blank_audio]", "[music]", "[silence]", "[noise]", "[laughter]",
        "[applause]", "[cough]", "[inaudible]", "[ blank_audio ]",
        "subtitles by", "www.", ".com", "thank you for watching",
        "thank you.", "you",
    ];
    let lower = t.to_lowercase();
    if hallucinations.iter().any(|h| lower == *h) {
        return String::new();
    }

    // Whisper wraps ambient sound descriptions in () or [] or **
    // e.g. "(motor running)", "[sound of birds]", "(dramatic music)"
    if (t.starts_with('(') && t.ends_with(')')) ||
       (t.starts_with('[') && t.ends_with(']')) ||
       (t.starts_with('*') && t.ends_with('*')) {
        return String::new();
    }

    // Too short to be real speech
    if t.len() < 4 { return String::new(); }

    t.to_string()
}

/// Holds the running audio capture. Dropping this stops the mic stream
/// and closes the audio channel, which shuts down the STT loop.
pub struct VoiceHandle {
    pub capture: AudioCapture,
}

/// Start mic capture. Returns the handle (keeps stream alive) and the
/// audio frame receiver (pass to run_stt_loop).
pub fn start_capture() -> Result<(VoiceHandle, mpsc::Receiver<Vec<f32>>)> {
    let (tx, rx) = mpsc::channel(AUDIO_CHANNEL_BUF);
    // Use a throwaway energy arc here — start_capture is not the live voice path
    let capture = AudioCapture::start(tx, std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)))?;
    Ok((VoiceHandle { capture }, rx))
}

/// Run the STT loop: accumulate VAD frames, transcribe on silence,
/// emit `voice_transcript` events to the frontend.
/// Exits when the audio channel closes (i.e. VoiceHandle is dropped).
pub async fn run_stt_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    stt_port: u16,
    sample_rate: u32,
    channels: u16,
    app_handle: tauri::AppHandle,
) {
    let stt = SttClient::new(stt_port);
    let mut buffer: Vec<f32> = Vec::new();

    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(SILENCE_MS),
            audio_rx.recv(),
        ).await {
            Ok(Some(frame)) => buffer.extend_from_slice(&frame),
            Ok(None) => break, // channel closed
            Err(_) => {
                // Silence gap — require at least 0.4s of audio (avoids blank clips)
                let min_samples = sample_rate as usize * 2 / 5; // 0.4 seconds
                if buffer.len() >= min_samples {
                    // Downmix to mono + resample to 16 kHz with rubato sinc resampler.
                    // Sending the model exactly the rate it was trained on (16 kHz)
                    // rather than relying on Parakeet's internal Python resampling.
                    let prepared = prepare_for_stt(&buffer, sample_rate, channels);
                    let boosted = amplify(&prepared, MIC_GAIN);
                    match stt.transcribe(&boosted, crate::constants::STT_SAMPLE_RATE, 1).await {
                        Ok(text) => {
                            let cleaned = clean_transcript(&text);
                            if !cleaned.is_empty() {
                                let _ = app_handle.emit("voice_transcript", cleaned);
                            }
                        }
                        Err(e) => eprintln!("[STT] transcribe error: {e}"),
                    }
                    buffer.clear();
                }
            }
        }
    }
}

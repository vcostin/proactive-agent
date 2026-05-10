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
/// rubato's sinc resampler — high quality, handles any device rate.
/// Returns (resampled_audio, true) on success or (raw_mono, false) on error.
fn prepare_for_stt(samples: &[f32], device_rate: u32, channels: u16) -> (Vec<f32>, bool) {
    use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType,
                  WindowFunction, Resampler};
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
        return (mono, true);
    }

    let params = SincInterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 64,
        window: WindowFunction::BlackmanHarris2,
    };

    let ratio = STT_SAMPLE_RATE as f64 / device_rate as f64;
    let mut resampler = match SincFixedIn::<f32>::new(
        ratio,
        2.0,    // max_relative_ratio
        params,
        mono.len(),
        1,      // mono
    ) {
        Ok(r) => r,
        Err(e) => return (mono, false),
    };

    match resampler.process(&[mono.clone()], None) {
        Ok(out) => (out.into_iter().next().unwrap_or_default(), true),
        Err(_e) => (mono, false),
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
    debug_event(&app_handle, format!(
        "STT loop started — device: {sample_rate} Hz {channels}ch → target: {} Hz mono",
        crate::constants::STT_SAMPLE_RATE
    ));

    let stt = SttClient::new(stt_port);
    let mut buffer: Vec<f32> = Vec::new();

    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(SILENCE_MS),
            audio_rx.recv(),
        ).await {
            Ok(Some(frame)) => buffer.extend_from_slice(&frame),
            Ok(None) => break, // channel closed — voice stopped
            Err(_) => {
                // Silence gap — require at least 0.4s of audio
                let min_samples = sample_rate as usize * 2 / 5;
                if buffer.len() >= min_samples {
                    // Run CPU-bound rubato resampling off the tokio executor thread
                    let buf = buffer.clone();
                    let app = app_handle.clone();
                    let (prepared, resampled_ok) = tokio::task::spawn_blocking(
                        move || prepare_for_stt(&buf, sample_rate, channels)
                    ).await.unwrap_or_else(|_| (buffer.clone(), false));

                    if !resampled_ok {
                        debug_event(&app_handle, format!(
                            "rubato failed for {sample_rate}→{} Hz — falling back to native rate",
                            crate::constants::STT_SAMPLE_RATE
                        ));
                    }

                    let out_rate = if resampled_ok {
                        crate::constants::STT_SAMPLE_RATE
                    } else {
                        sample_rate
                    };

                    let boosted = amplify(&prepared, MIC_GAIN);
                    match stt.transcribe(&boosted, out_rate, 1).await {
                        Ok(text) => {
                            let cleaned = clean_transcript(&text);
                            if !cleaned.is_empty() {
                                let _ = app_handle.emit("voice_transcript", cleaned);
                            }
                        }
                        Err(e) => debug_event(&app_handle, format!("STT transcribe error: {e}")),
                    }
                    buffer.clear();
                }
            }
        }
    }

    debug_event(&app_handle, "STT loop exited".to_string());
}

/// Emit a debug event synchronously from async context.
fn debug_event(app: &tauri::AppHandle, message: String) {
    use tauri::Emitter;
    let _ = app.emit("debug_event", crate::monitor::DebugEvent {
        timestamp: chrono::Utc::now(),
        component: "[AUDIO]".to_string(),
        message,
    });
}

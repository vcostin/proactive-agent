#![allow(dead_code, unused_imports)]
pub mod capture;
pub mod piper_catalog;
pub mod piper_download;
pub mod piper_voice;
pub mod stt;
pub mod tts;

pub use capture::{
    frame_rms, pick_input_config, quiet_backend_probe_noise, resolve_input_device,
    resolve_output_device, AudioCapture,
};
pub use piper_catalog::{list_curated_piper_voices, CuratedPiperVoice};
pub use piper_download::{
    curated_voice_download_urls, download_curated_piper_voice, HttpVoiceFileFetcher,
    VoiceDownloadProgress, VoiceFileFetcher, PIPER_VOICES_HF_V1,
};
pub use stt::SttClient;
pub use tts::TtsClient;

use anyhow::Result;
use tauri::Emitter;
use tokio::sync::mpsc;

const AUDIO_CHANNEL_BUF: usize = 256;
const SILENCE_MS: u64 = 1000; // 1s silence before sending — captures complete sentences
const MIC_GAIN: f32 = 1.5;   // kept for API compat, replaced by normalization in amplify()
/// Keep this much audio before/after the energy island (protects plosive onsets).
const TRIM_PAD_MS: u32 = 200;
/// Relative RMS floor vs peak when locating speech for trim.
const TRIM_REL_THR: f32 = 0.15;

/// Normalize audio to a target peak level regardless of mic volume.
/// Better than fixed gain — adapts to the actual signal level automatically.
fn amplify(samples: &[f32], _gain: f32) -> Vec<f32> {
    const TARGET_PEAK: f32 = 0.7; // -3 dBFS — good level for ASR models
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak < 0.001 { return samples.to_vec(); } // silence — nothing to boost
    let gain = (TARGET_PEAK / peak).min(20.0); // cap at 26dB to avoid noise amplification
    samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Drop long leading/trailing quiet so greedy TDT is not dominated by silence
/// pads (isolated short words like "face" otherwise decode empty).
fn trim_silence(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() || sample_rate == 0 {
        return samples.to_vec();
    }
    let win = ((sample_rate as usize) / 50).max(1); // ~20 ms
    let energies: Vec<f32> = samples
        .chunks(win)
        .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = energies.iter().copied().fold(0.0f32, f32::max);
    if peak < 0.001 {
        return samples.to_vec();
    }
    let thr = (peak * TRIM_REL_THR).max(0.005);
    let active: Vec<usize> = energies
        .iter()
        .enumerate()
        .filter(|(_, e)| **e >= thr)
        .map(|(i, _)| i)
        .collect();
    let Some(&first) = active.first() else {
        return samples.to_vec();
    };
    let last = *active.last().unwrap_or(&first);
    let pad_wins = (sample_rate.saturating_mul(TRIM_PAD_MS) / 1000) as usize / win;
    let start = first.saturating_sub(pad_wins) * win;
    let end = ((last + 1 + pad_wins) * win).min(samples.len());
    if start == 0 && end == samples.len() {
        return samples.to_vec();
    }
    samples[start..end].to_vec()
}

/// Downmix stereo→mono then resample to STT_SAMPLE_RATE (16 kHz) using
/// rubato's sinc resampler — high quality, handles any device rate.
/// Returns (resampled_audio, true) on success or (raw_mono, false) on error.
fn prepare_for_stt(samples: &[f32], device_rate: u32, channels: u16) -> (Vec<f32>, bool) {
    use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType,
                  WindowFunction, Resampler};
    use crate::constants::STT_SAMPLE_RATE;

    // Step 1: downmix to mono — take the louder channel (not average).
    // Averaging near-duplicate stereo from a mono USB mic can still notch
    // high frequencies when channels are slightly out of phase.
    let mono: Vec<f32> = if channels <= 1 {
        samples.to_vec()
    } else {
        let ch = channels as usize;
        let mut left_e = 0.0f32;
        let mut right_e = 0.0f32;
        for frame in samples.chunks(ch) {
            if !frame.is_empty() {
                left_e += frame[0] * frame[0];
            }
            if frame.len() > 1 {
                right_e += frame[1] * frame[1];
            }
        }
        let take = if right_e > left_e { 1 } else { 0 };
        samples
            .chunks(ch)
            .filter_map(|frame| frame.get(take).copied())
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
        Err(_e) => return (mono, false),
    };

    match resampler.process(&[mono.clone()], None) {
        Ok(out) => (out.into_iter().next().unwrap_or_default(), true),
        Err(_e) => (mono, false),
    }
}

/// Filter out common STT hallucinations for non-speech audio.
/// Returns empty string if the transcript should be discarded.
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

    // Keep short real words ("ok", "no", "Si") — only drop near-empty noise
    // like "F." / "." that used to fail the old `len < 4` gate.
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return String::new();
    }

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
///
/// When `stt` is `None`, transcription stays off (soft-fail / not ready) while
/// the mic path can still deliver frames for waveform/debug.
pub async fn run_stt_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    stt: Option<std::sync::Arc<SttClient>>,
    sample_rate: u32,
    channels: u16,
    app_handle: tauri::AppHandle,
) {
    debug_event(&app_handle, format!(
        "STT loop started — device: {sample_rate} Hz {channels}ch → target: {} Hz mono; engine={}",
        crate::constants::STT_SAMPLE_RATE,
        if stt.is_some() { "in-process ort" } else { "unavailable (transcription off)" }
    ));

    let mut buffer: Vec<f32> = Vec::new();
    let mut frames_received: u64 = 0;
    let mut last_frame_log = std::time::Instant::now();

    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(SILENCE_MS),
            audio_rx.recv(),
        ).await {
            Ok(Some(frame)) => {
                frames_received += 1;
                buffer.extend_from_slice(&frame);
                // Log frame activity every 5 seconds so we can confirm VAD is triggering
                if last_frame_log.elapsed().as_secs() >= 5 {
                    debug_event(&app_handle, format!(
                        "VAD active — {frames_received} frames received ({} samples buffered)",
                        buffer.len()
                    ));
                    last_frame_log = std::time::Instant::now();
                }
            }
            Ok(None) => break, // channel closed — voice stopped
            Err(_) => {
                // Silence gap — require at least 0.4s of audio
                let min_samples = sample_rate as usize * 2 / 5;
                if buffer.len() >= min_samples {
                    // Run CPU-bound rubato resampling off the tokio executor thread
                    let buf = buffer.clone();
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

                    let boosted = amplify(&trim_silence(&prepared, out_rate), MIC_GAIN);
                    if let Some(client) = stt.clone() {
                        let audio = boosted.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            client.transcribe(&audio, out_rate, 1)
                        })
                        .await;
                        match result {
                            Ok(Ok(text)) => {
                                let cleaned = clean_transcript(&text);
                                if !cleaned.is_empty() {
                                    let _ = app_handle.emit("voice_transcript", cleaned);
                                } else if !text.trim().is_empty() {
                                    debug_event(&app_handle, format!(
                                        "STT discarded as hallucination/noise: {text:?}"
                                    ));
                                }
                            }
                            Ok(Err(e)) => {
                                let msg = format!("STT transcribe error: {e:#}");
                                debug_event(&app_handle, msg.clone());
                                let _ = app_handle.emit("voice_error", msg);
                            }
                            Err(e) => {
                                let msg = format!("STT blocking task join error: {e}");
                                debug_event(&app_handle, msg.clone());
                                let _ = app_handle.emit("voice_error", msg);
                            }
                        }
                    } else {
                        debug_event(
                            &app_handle,
                            "STT utterance skipped — engine unavailable (Setup repair)".into(),
                        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_transcript_keeps_short_words() {
        assert_eq!(clean_transcript("ok"), "ok");
        assert_eq!(clean_transcript("no"), "no");
        assert_eq!(clean_transcript("Si"), "Si");
        assert_eq!(clean_transcript("Face."), "Face.");
    }

    #[test]
    fn clean_transcript_drops_near_empty_noise() {
        assert!(clean_transcript("F.").is_empty());
        assert!(clean_transcript(".").is_empty());
        assert!(clean_transcript("you").is_empty());
        assert!(clean_transcript("[music]").is_empty());
    }

    #[test]
    fn trim_silence_removes_long_pads_keeps_speech() {
        let sr = 16_000u32;
        let pad = vec![0.0f32; sr as usize]; // 1s silence
        let mut speech = vec![0.0f32; sr as usize / 2]; // 0.5s
        for (i, s) in speech.iter_mut().enumerate() {
            // audible tone
            *s = 0.2 * (2.0 * std::f32::consts::PI * 440.0 * (i as f32) / sr as f32).sin();
        }
        let mut full = pad.clone();
        full.extend_from_slice(&speech);
        full.extend_from_slice(&pad);

        let trimmed = trim_silence(&full, sr);
        assert!(
            trimmed.len() < full.len(),
            "expected trim to shorten {} → {}",
            full.len(),
            trimmed.len()
        );
        // ~0.5s speech + 200ms pad each side ≈ 0.9s; allow slack
        let secs = trimmed.len() as f32 / sr as f32;
        assert!(
            (0.7..1.3).contains(&secs),
            "trimmed duration {secs}s out of expected band"
        );
        let peak = trimmed.iter().copied().map(f32::abs).fold(0.0f32, f32::max);
        assert!(peak > 0.1, "speech peak must survive trim");
    }
}

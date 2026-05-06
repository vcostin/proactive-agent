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
const SILENCE_MS: u64 = 800;
/// Gain multiplier applied before sending audio to Whisper.
/// 1.0 = no boost. Too high causes clipping which garbles transcription.
const MIC_GAIN: f32 = 1.5;

/// Filter out common whisper hallucinations for non-speech audio.
/// Returns empty string if the transcript should be discarded.
/// Amplify PCM samples by `gain`, clamped to [-1.0, 1.0] to avoid clipping.
fn amplify(samples: &[f32], gain: f32) -> Vec<f32> {
    samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
}

/// Downmix stereo→mono and resample to 16000 Hz.
/// Whisper only processes 16000 Hz mono — sending 48000 Hz stereo
/// makes it hear everything at 3× wrong speed and in double.
fn prepare_for_whisper(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<f32> {
    // Step 1: downmix to mono
    let mono: Vec<f32> = if channels <= 1 {
        samples.to_vec()
    } else {
        let ch = channels as usize;
        samples.chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    // Step 2: resample to 16000 Hz via linear interpolation
    const WHISPER_RATE: u32 = 16000;
    if sample_rate == WHISPER_RATE {
        return mono;
    }
    let ratio = sample_rate as f64 / WHISPER_RATE as f64;
    let out_len = (mono.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = mono.get(idx).copied().unwrap_or(0.0);
        let s1 = mono.get(idx + 1).copied().unwrap_or(0.0);
        out.push(s0 + (s1 - s0) * frac);
    }
    out
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
    let capture = AudioCapture::start(tx)?;
    Ok((VoiceHandle { capture }, rx))
}

/// Run the STT loop: accumulate VAD frames, transcribe on silence,
/// emit `voice_transcript` events to the frontend.
/// Exits when the audio channel closes (i.e. VoiceHandle is dropped).
pub async fn run_stt_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    model_path: std::path::PathBuf,
    sample_rate: u32,
    channels: u16,
    app_handle: tauri::AppHandle,
) {
    let stt = SttClient::new(model_path);
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
                    // Resample + downmix to 16000 Hz mono (Whisper's required format)
                    let mono16k = prepare_for_whisper(&buffer, sample_rate, channels);
                    // Then boost the already-correct-rate signal
                    let boosted = amplify(&mono16k, MIC_GAIN);
                    // Always send as 16000 Hz mono after resampling
                    match stt.transcribe(&boosted, 16000, 1).await {
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

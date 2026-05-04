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
/// 2.0 = +6 dB, 3.0 = +9.5 dB, 4.0 = +12 dB. Tune if mic is still too quiet.
const MIC_GAIN: f32 = 2.5;

/// Filter out common whisper hallucinations for non-speech audio.
/// Returns empty string if the transcript should be discarded.
/// Amplify PCM samples by `gain`, clamped to [-1.0, 1.0] to avoid clipping.
fn amplify(samples: &[f32], gain: f32) -> Vec<f32> {
    samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
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
    whisper_port: u16,
    sample_rate: u32,
    channels: u16,
    app_handle: tauri::AppHandle,
) {
    let stt = SttClient::new(whisper_port);
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
                    // Boost mic input before transcription (compensates for low mic level)
                    let boosted = amplify(&buffer, MIC_GAIN);
                    match stt.transcribe(&boosted, sample_rate, channels).await {
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

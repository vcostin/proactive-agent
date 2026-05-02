pub mod capture;
pub mod stt;
pub mod tts;

pub use capture::AudioCapture;
pub use stt::SttClient;
pub use tts::TtsClient;

use anyhow::Result;
use tokio::sync::mpsc;

/// Channel buffer for audio frames between capture and STT.
const AUDIO_CHANNEL_BUF: usize = 128;

/// Convenience wrapper — starts capture and returns the capture handle plus
/// the channel receiver the STT loop should read from.
pub fn start_capture() -> Result<(AudioCapture, mpsc::Receiver<Vec<f32>>)> {
    let (tx, rx) = mpsc::channel(AUDIO_CHANNEL_BUF);
    let capture = AudioCapture::start(tx)?;
    Ok((capture, rx))
}

/// Background STT loop: accumulates audio frames while VAD is active, then
/// sends the batch to Whisper when silence is detected.
/// EXTEND Phase 4: route transcript into orchestrator.send_message()
pub async fn run_stt_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    stt: SttClient,
    sample_rate: u32,
    channels: u16,
) {
    const SILENCE_DRAIN_MS: u64 = 400;
    let mut buffer: Vec<f32> = Vec::new();

    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(SILENCE_DRAIN_MS),
            audio_rx.recv(),
        )
        .await
        {
            Ok(Some(frame)) => {
                // Receiving frames — VAD active, accumulate
                buffer.extend_from_slice(&frame);
            }
            Ok(None) => break, // channel closed
            Err(_) => {
                // Timeout = silence gap — send accumulated audio if any
                if !buffer.is_empty() {
                    match stt.transcribe(&buffer, sample_rate, channels).await {
                        Ok(text) if !text.is_empty() => {
                            // EXTEND: send `text` to orchestrator
                            println!("[STT] transcript: {text}");
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("[STT] error: {e}"),
                    }
                    buffer.clear();
                }
            }
        }
    }
}

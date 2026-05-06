#![allow(dead_code)]
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// RMS level above which audio is considered speech.
/// Keep low — false positives are filtered by clean_transcript().
/// Better to capture too much than miss speech entirely.
const VAD_THRESHOLD: f32 = 0.008;

pub struct AudioCapture {
    /// Kept alive — stream stops when this is dropped.
    _stream: cpal::Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// true while speech energy is detected.
    pub vad_active: Arc<AtomicBool>,
    /// Current RMS energy, stored as f32 bits in an AtomicU32.
    energy_bits: Arc<AtomicU32>,
}

impl AudioCapture {
    /// Start mic capture. `energy_out` is updated continuously with the current
    /// RMS level so the UI can animate a live waveform without polling the struct.
    pub fn start(audio_tx: mpsc::Sender<Vec<f32>>, energy_out: Arc<AtomicU32>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no audio input device found")?;

        let supported = device
            .default_input_config()
            .context("cannot get default input config")?;

        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        // Use device native config — not all devices support forced mono
        // prepare_for_whisper() handles the stereo→mono downmix before transcription
        let config: cpal::StreamConfig = supported.into();

        let vad_active = Arc::new(AtomicBool::new(false));
        let energy_bits = Arc::new(AtomicU32::new(0));
        let vad_clone = vad_active.clone();
        let energy_clone = energy_bits.clone();
        // Also update the shared energy so the UI can animate in real-time
        let energy_shared = energy_out;

        let stream = device
            .build_input_stream::<f32, _, _>(
                &config,
                move |data: &[f32], _| {
                    let rms = (data.iter().map(|&s| s * s).sum::<f32>()
                        / data.len() as f32)
                        .sqrt();

                    energy_clone.store(rms.to_bits(), Ordering::Relaxed);
                    energy_shared.store(rms.to_bits(), Ordering::Relaxed);
                    let speech = rms > VAD_THRESHOLD;
                    vad_clone.store(speech, Ordering::Relaxed);

                    if speech {
                        // Non-blocking — drop the frame if the receiver is busy
                        let _ = audio_tx.try_send(data.to_vec());
                    }
                },
                |err| eprintln!("[AUDIO] capture error: {err}"),
                None,
            )
            .context("failed to build input stream")?;

        stream.play().context("failed to start capture")?;

        Ok(Self {
            _stream: stream,
            device_name,
            sample_rate,
            channels,
            vad_active,
            energy_bits,
        })
    }

    pub fn energy(&self) -> f32 {
        f32::from_bits(self.energy_bits.load(Ordering::Relaxed))
    }

    pub fn is_active(&self) -> bool {
        self.vad_active.load(Ordering::Relaxed)
    }
}

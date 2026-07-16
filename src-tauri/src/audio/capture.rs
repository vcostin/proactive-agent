#![allow(dead_code)]
use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// RMS level above which audio is considered speech.
/// Low threshold = capture more, rely on STT to ignore noise.
/// Missing word beginnings is worse than sending a bit of silence.
const VAD_THRESHOLD: f32 = 0.005;

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
        let device = resolve_input_device(&host)?;

        let supported = device
            .default_input_config()
            .context("cannot get default input config")?;

        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let channels_usize = channels as usize;
        // Use device native config — not all devices support forced mono
        // prepare_for_stt() handles the stereo→mono downmix before transcription
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
                    let rms = frame_rms(data, channels_usize);

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

/// Suppress ALSA/JACK probe spam that cpal triggers when enumerating devices.
///
/// Under PipeWire, listing ALSA PCMs tries jack/oss/dsnoop/route plugins; those
/// write expected failures to stderr. Call once at process start; device resolve
/// also scopes a stderr mute around enumeration.
pub fn quiet_backend_probe_noise() {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("JACK_NO_START_SERVER").is_none() {
            // SAFETY: called once during early process init before audio threads start.
            unsafe { std::env::set_var("JACK_NO_START_SERVER", "1") };
        }
    }
}

/// Mute stderr for the lifetime of this guard (Linux). Restores on drop.
#[cfg(target_os = "linux")]
struct StderrMute {
    saved_fd: i32,
}

#[cfg(target_os = "linux")]
impl StderrMute {
    fn enter() -> Option<Self> {
        // SAFETY: POSIX dup/dup2/open; we only touch fd 2 for this process.
        unsafe {
            let saved = libc::dup(2);
            if saved < 0 {
                return None;
            }
            let devnull = libc::open(b"/dev/null\0".as_ptr() as *const _, libc::O_WRONLY);
            if devnull < 0 {
                libc::close(saved);
                return None;
            }
            if libc::dup2(devnull, 2) < 0 {
                libc::close(devnull);
                libc::close(saved);
                return None;
            }
            libc::close(devnull);
            Some(Self { saved_fd: saved })
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for StderrMute {
    fn drop(&mut self) {
        unsafe {
            libc::dup2(self.saved_fd, 2);
            libc::close(self.saved_fd);
        }
    }
}

fn with_probe_muted<T>(f: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "linux")]
    let _mute = StderrMute::enter();
    f()
}

/// Prefer PipeWire/Pulse PCMs for output too (same ALSA `default` footgun).
pub fn resolve_output_device(host: &cpal::Host) -> Result<cpal::Device> {
    const PREFERRED: &[&str] = &["pulse", "pipewire"];
    with_probe_muted(|| {
        let mut devices: Vec<cpal::Device> = match host.output_devices() {
            Ok(devs) => devs.collect(),
            Err(_) => Vec::new(),
        };
        for want in PREFERRED {
            if let Some(i) = devices
                .iter()
                .position(|d| d.name().ok().as_deref() == Some(*want))
            {
                if devices[i].default_output_config().is_ok() {
                    return Ok(devices.swap_remove(i));
                }
            }
        }
        host.default_output_device()
            .context("no audio output device found")
    })
}

/// Pick a usable input device under PipeWire/Pulse.
///
/// On Linux, cpal's ALSA host often returns a logical device named `default` that
/// routes through `dsnoop` and fails with "unable to open slave" while the
/// `pulse` / `pipewire` PCMs work. Prefer those when they open cleanly.
pub fn resolve_input_device(host: &cpal::Host) -> Result<cpal::Device> {
    const PREFERRED: &[&str] = &["pulse", "pipewire"];

    with_probe_muted(|| {
        let mut devices: Vec<cpal::Device> = match host.input_devices() {
            Ok(devs) => devs.collect(),
            Err(_) => Vec::new(),
        };

        for want in PREFERRED {
            if let Some(i) = devices
                .iter()
                .position(|d| d.name().ok().as_deref() == Some(*want))
            {
                if devices[i].default_input_config().is_ok() {
                    return Ok(devices.swap_remove(i));
                }
            }
        }

        if let Some(d) = host.default_input_device() {
            if d.default_input_config().is_ok() {
                return Ok(d);
            }
        }

        if let Some(i) = devices.iter().position(|d| d.default_input_config().is_ok()) {
            return Ok(devices.swap_remove(i));
        }

        bail!(
            "no usable audio input device (ALSA default/dsnoop may be broken under PipeWire — \
             tried pulse/pipewire fallbacks)"
        )
    })
}

/// Peak per-channel RMS so mono mics presented as stereo aren't diluted by a silent channel.
pub fn frame_rms(data: &[f32], channels: usize) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    if channels <= 1 {
        return (data.iter().map(|&s| s * s).sum::<f32>() / data.len() as f32).sqrt();
    }
    let mut best = 0.0f32;
    for ch in 0..channels {
        let mut sum = 0.0f32;
        let mut n = 0usize;
        let mut i = ch;
        while i < data.len() {
            sum += data[i] * data[i];
            n += 1;
            i += channels;
        }
        if n > 0 {
            best = best.max((sum / n as f32).sqrt());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rms_mono_matches_naive() {
        let data = [0.0f32, 0.5, -0.5, 0.0];
        let naive = (data.iter().map(|&s| s * s).sum::<f32>() / data.len() as f32).sqrt();
        assert!((frame_rms(&data, 1) - naive).abs() < 1e-6);
    }

    #[test]
    fn frame_rms_stereo_uses_loudest_channel() {
        // Left silent, right has energy — interleaved L,R,L,R…
        let data = [0.0f32, 0.5, 0.0, 0.5];
        let left_only = frame_rms(&[0.0, 0.0], 1);
        let right = frame_rms(&[0.5, 0.5], 1);
        let stereo = frame_rms(&data, 2);
        assert!(stereo > left_only);
        assert!((stereo - right).abs() < 1e-6);
    }
}

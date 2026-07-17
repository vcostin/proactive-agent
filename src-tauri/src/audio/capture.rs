#![allow(dead_code)]
use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, SupportedStreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::constants::STT_SAMPLE_RATE;

/// RMS level above which audio is considered speech.
/// Low threshold = capture more, rely on STT to ignore noise.
/// Missing word beginnings is worse than sending a bit of silence.
const VAD_THRESHOLD: f32 = 0.005;
/// Pre-roll / hangover around speech so quiet fricative onsets (`/f/`) are kept.
const VAD_PAD_MS: u32 = 200;

/// Energy VAD with ring-buffer pre-roll and hangover post-roll.
///
/// Only frames that pass the gate are returned from [`VadGate::push`]; the
/// caller forwards them into the STT utterance buffer.
struct VadGate {
    threshold: f32,
    pad_samples: usize,
    preroll: VecDeque<Vec<f32>>,
    preroll_samples: usize,
    hangover_left: usize,
    in_utterance: bool,
}

impl VadGate {
    fn new(sample_rate: u32, channels: usize) -> Self {
        let pad_samples = (sample_rate as usize)
            .saturating_mul(VAD_PAD_MS as usize)
            .saturating_mul(channels.max(1))
            / 1000;
        Self {
            threshold: VAD_THRESHOLD,
            pad_samples: pad_samples.max(1),
            preroll: VecDeque::new(),
            preroll_samples: 0,
            hangover_left: 0,
            in_utterance: false,
        }
    }

    /// Feed one capture callback. Returns the frames that should be sent to STT
    /// (possibly empty, possibly including flushed pre-roll).
    fn push(&mut self, frame: Vec<f32>, rms: f32) -> Vec<Vec<f32>> {
        let speech = rms > self.threshold;
        let mut out = Vec::new();

        if speech {
            if !self.in_utterance {
                while let Some(pre) = self.preroll.pop_front() {
                    self.preroll_samples = self.preroll_samples.saturating_sub(pre.len());
                    out.push(pre);
                }
                self.preroll_samples = 0;
            }
            self.in_utterance = true;
            self.hangover_left = self.pad_samples;
            out.push(frame);
            return out;
        }

        if self.hangover_left > 0 {
            let n = frame.len();
            out.push(frame);
            self.hangover_left = self.hangover_left.saturating_sub(n);
            if self.hangover_left == 0 {
                self.in_utterance = false;
            }
            return out;
        }

        self.in_utterance = false;
        self.preroll_samples += frame.len();
        self.preroll.push_back(frame);
        while self.preroll_samples > self.pad_samples {
            if let Some(old) = self.preroll.pop_front() {
                self.preroll_samples = self.preroll_samples.saturating_sub(old.len());
            } else {
                break;
            }
        }
        out
    }

    fn is_active(&self) -> bool {
        self.in_utterance || self.hangover_left > 0
    }
}

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

        let supported = pick_input_config(&device)?;

        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let channels_usize = channels as usize;
        let sample_format = supported.sample_format();
        // prepare_for_stt() handles stereo→mono and rate conversion when needed
        let config: cpal::StreamConfig = supported.into();

        // Visible in the tauri-dev terminal so Host STT capture format is checkable.
        eprintln!(
            "[AUDIO] capture config: {sample_rate} Hz, {channels} ch, {sample_format:?} ({device_name})"
        );

        let vad_active = Arc::new(AtomicBool::new(false));
        let energy_bits = Arc::new(AtomicU32::new(0));
        let vad_clone = vad_active.clone();
        let energy_clone = energy_bits.clone();
        let energy_shared = energy_out;
        let mut gate = VadGate::new(sample_rate, channels_usize);

        // We only build an f32 stream — pick_input_config prefers F32.
        if sample_format != SampleFormat::F32 {
            bail!("expected F32 capture format, got {sample_format:?}");
        }

        let stream = device
            .build_input_stream::<f32, _, _>(
                &config,
                move |data: &[f32], _| {
                    let rms = frame_rms(data, channels_usize);

                    energy_clone.store(rms.to_bits(), Ordering::Relaxed);
                    energy_shared.store(rms.to_bits(), Ordering::Relaxed);

                    for frame in gate.push(data.to_vec(), rms) {
                        let _ = audio_tx.try_send(frame);
                    }
                    vad_clone.store(gate.is_active(), Ordering::Relaxed);
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

/// Choose a capture config that minimizes resampling before STT.
///
/// Pulse's default is often 44100 stereo F32 while the USB mic runs at 48000.
/// Opening at 44100 forces PipeWire 48k→44.1k then our 44.1k→16k — double
/// resample that blurs fricatives (/f/ vs /s/). Prefer mono @ 16 kHz, then
/// mono @ 48 kHz, then mono @ 44.1 kHz, then the device default.
pub fn pick_input_config(device: &cpal::Device) -> Result<SupportedStreamConfig> {
    with_probe_muted(|| {
        let preferred_rates = [STT_SAMPLE_RATE, 48_000, 44_100];
        if let Ok(ranges) = device.supported_input_configs() {
            let ranges: Vec<_> = ranges.collect();
            // 1) mono F32 at preferred rates
            for &rate in &preferred_rates {
                for range in &ranges {
                    if range.channels() == 1
                        && range.sample_format() == SampleFormat::F32
                        && range.min_sample_rate().0 <= rate
                        && range.max_sample_rate().0 >= rate
                    {
                        return Ok(range.clone().with_sample_rate(SampleRate(rate)));
                    }
                }
            }
            // 2) stereo F32 at 48 kHz / 16 kHz (better than default 44.1k)
            for &rate in &[48_000u32, STT_SAMPLE_RATE] {
                for range in &ranges {
                    if range.channels() == 2
                        && range.sample_format() == SampleFormat::F32
                        && range.min_sample_rate().0 <= rate
                        && range.max_sample_rate().0 >= rate
                    {
                        return Ok(range.clone().with_sample_rate(SampleRate(rate)));
                    }
                }
            }
        }
        device
            .default_input_config()
            .context("cannot get default input config")
    })
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

    #[test]
    fn vad_gate_flushes_preroll_on_speech_onset() {
        // pad = 200ms @ 1 kHz mono = 200 samples — tiny rate for a short test
        let mut gate = VadGate::new(1_000, 1);
        let quiet = vec![0.0f32; 50];
        let loud = vec![0.2f32; 50];

        assert!(gate.push(quiet.clone(), 0.0).is_empty());
        assert!(gate.push(quiet.clone(), 0.0).is_empty());
        assert!(gate.push(quiet.clone(), 0.0).is_empty());
        assert!(gate.push(quiet, 0.0).is_empty());

        let sent = gate.push(loud.clone(), 0.2);
        // Pre-roll (~200 samples) + current frame
        let total: usize = sent.iter().map(|f| f.len()).sum();
        assert!(
            total >= 200 + 50,
            "expected preroll+frame, got {total} samples across {} frames",
            sent.len()
        );
        assert!(gate.is_active());
    }

    #[test]
    fn vad_gate_hangover_keeps_trailing_frames() {
        let mut gate = VadGate::new(1_000, 1);
        let loud = vec![0.2f32; 50];
        let quiet = vec![0.0f32; 50];

        let _ = gate.push(loud, 0.2);
        // Below threshold but hangover still open
        let sent = gate.push(quiet.clone(), 0.0);
        assert_eq!(sent.len(), 1);
        assert!(gate.is_active());

        // Drain hangover (200 samples @ 1kHz → 4×50)
        let mut n = 0;
        for _ in 0..10 {
            let s = gate.push(quiet.clone(), 0.0);
            n += s.len();
            if !gate.is_active() {
                break;
            }
        }
        assert!(n >= 1, "hangover should forward quiet frames");
        assert!(!gate.is_active());
    }

    #[test]
    fn pick_input_config_prefers_mono_stt_rate_when_advertised() {
        // Pulse advertises 1..=768000 for mono F32 — we must not keep 44100 stereo default.
        quiet_backend_probe_noise();
        let host = cpal::default_host();
        let Ok(device) = resolve_input_device(&host) else {
            // No mic in CI — skip
            return;
        };
        let Ok(cfg) = pick_input_config(&device) else {
            return;
        };
        assert_eq!(cfg.sample_format(), SampleFormat::F32);
        // Prefer mono @ 16 kHz when the device claims to support it (Pulse does).
        if cfg.channels() == 1 {
            assert_eq!(cfg.sample_rate().0, STT_SAMPLE_RATE);
        } else {
            // Fallback path: at least avoid the bad 44100 default when 48k is available.
            assert_ne!(
                (cfg.channels(), cfg.sample_rate().0),
                (2, 44_100),
                "should not keep Pulse default 44100 stereo"
            );
        }
    }
}

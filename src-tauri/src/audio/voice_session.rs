//! VoiceSession — Host mic capture lifecycle behind one deep module.
//!
//! Callers start/stop (or Drop) a session. They do not own thread spawn,
//! mpsc wiring, or stop-flag swap. Soft-fail when Host STT is unavailable
//! keeps capture up with transcription off and a Setup repair hint.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use super::capture::AudioCapture;
use super::stt::SttClient;

const AUDIO_CHANNEL_BUF: usize = 128;
const FORMAT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_CHANNELS: u16 = 1;

/// Soft-fail / capture diagnostics — production may emit Tauri debug events.
pub trait SessionLog: Send + Sync {
    fn log(&self, message: &str);

    /// Host STT soft-fail / Setup repair hint (defaults to [`Self::log`]).
    fn soft_fail(&self, message: &str) {
        self.log(message);
    }
}

/// Emit debug events via Tauri. Capture lifecycle → `[AUDIO]`; soft-fail → `[STT]`.
pub struct DebugEventSessionLog {
    app: tauri::AppHandle,
}

impl DebugEventSessionLog {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }

    fn emit(&self, component: &str, message: &str) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "debug_event",
            crate::monitor::DebugEvent {
                timestamp: chrono::Utc::now(),
                component: component.to_string(),
                message: message.to_string(),
            },
        );
    }
}

impl SessionLog for DebugEventSessionLog {
    fn log(&self, message: &str) {
        self.emit("[AUDIO]", message);
    }

    fn soft_fail(&self, message: &str) {
        self.emit("[STT]", message);
    }
}

/// Whether the session will feed an STT engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionMode {
    /// Host STT engine present — utterance loop should transcribe.
    On,
    /// Soft-fail: mic/waveform up, transcription off (ADR 0001).
    Off,
}

/// Observable result of a successful [`VoiceSession::start`].
pub struct VoiceSessionStart {
    pub sample_rate: u32,
    pub channels: u16,
    pub audio_rx: mpsc::Receiver<Vec<f32>>,
    pub transcription: TranscriptionMode,
    /// Engine passed into start (`None` when soft-failed). Caller feeds `run_stt_loop`.
    pub stt: Option<Arc<SttClient>>,
}

/// Device (or fake) capture that runs on the dedicated capture thread.
///
/// Production: [`CpalCaptureBackend`]. Tests: inject a fake — no live mic.
pub trait CaptureBackend: Send + 'static {
    /// Block until `stop` is set. Call `report_format` once capture is live.
    /// Feed PCM frames to `audio_tx` and update `energy_out`.
    fn run(
        self: Box<Self>,
        audio_tx: mpsc::Sender<Vec<f32>>,
        energy_out: Arc<AtomicU32>,
        stop: Arc<AtomicBool>,
        report_format: Box<dyn FnOnce(u32, u16) + Send>,
        log: Arc<dyn SessionLog>,
    );
}

/// Live cpal mic capture (`AudioCapture` is `!Send` — stays on this thread).
pub struct CpalCaptureBackend;

impl CaptureBackend for CpalCaptureBackend {
    fn run(
        self: Box<Self>,
        audio_tx: mpsc::Sender<Vec<f32>>,
        energy_out: Arc<AtomicU32>,
        stop: Arc<AtomicBool>,
        report_format: Box<dyn FnOnce(u32, u16) + Send>,
        log: Arc<dyn SessionLog>,
    ) {
        match AudioCapture::start(audio_tx, energy_out) {
            Ok(capture) => {
                let sr = capture.sample_rate;
                let ch = capture.channels;
                log.log(&format!(
                    "capture started: {sr} Hz, {ch} ch — device: {}",
                    capture.device_name
                ));
                report_format(sr, ch);
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                log.log("capture stopped");
                // `capture` dropped here — stream tears down
            }
            Err(e) => {
                log.log(&format!("capture failed: {e}"));
            }
        }
    }
}

/// Owns the capture thread and stop flag. Dropping or [`Self::stop`] tears down
/// capture; both are idempotent.
pub struct VoiceSession {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    energy: Arc<AtomicU32>,
    stopped: bool,
}

impl VoiceSession {
    /// Start mic capture. When `stt` is `None`, soft-fails: capture still
    /// starts, transcription is [`TranscriptionMode::Off`], and `log`
    /// receives a Setup repair hint via [`SessionLog::soft_fail`].
    pub fn start(
        backend: Box<dyn CaptureBackend>,
        energy: Arc<AtomicU32>,
        log: Arc<dyn SessionLog>,
        stt: Option<Arc<SttClient>>,
    ) -> Result<(Self, VoiceSessionStart)> {
        let transcription = if stt.is_some() {
            TranscriptionMode::On
        } else {
            log.soft_fail(
                "Host STT engine unavailable — starting mic with transcription off. \
                 Open Setup Wizard / Setup repair to restore model + vocab + ONNX Runtime.",
            );
            TranscriptionMode::Off
        };

        let (tx, rx) = mpsc::channel::<Vec<f32>>(AUDIO_CHANNEL_BUF);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let energy_clone = energy.clone();
        let log_thread = log.clone();

        let (cfg_tx, cfg_rx) = std::sync::mpsc::channel::<(u32, u16)>();

        let thread_result = std::thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || {
                backend.run(
                    tx,
                    energy_clone,
                    stop_clone,
                    Box::new(move |sr, ch| {
                        let _ = cfg_tx.send((sr, ch));
                    }),
                    log_thread,
                );
            })
            .context("failed to spawn audio thread")?;

        let (sample_rate, channels) = cfg_rx
            .recv_timeout(FORMAT_HANDSHAKE_TIMEOUT)
            .unwrap_or((DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS));

        let session = Self {
            stop,
            join: Some(thread_result),
            energy,
            stopped: false,
        };

        Ok((
            session,
            VoiceSessionStart {
                sample_rate,
                channels,
                audio_rx: rx,
                transcription,
                stt,
            },
        ))
    }

    /// Signal the capture thread to exit and join it. Safe to call more than once.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
        self.energy.store(0u32, Ordering::Relaxed);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
}

impl Drop for VoiceSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingLog {
        lines: Mutex<Vec<String>>,
        soft_fail_lines: Mutex<Vec<String>>,
    }

    impl RecordingLog {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                lines: Mutex::new(Vec::new()),
                soft_fail_lines: Mutex::new(Vec::new()),
            })
        }

        fn soft_fail_joined(&self) -> String {
            self.soft_fail_lines.lock().unwrap().join("\n")
        }
    }

    impl SessionLog for RecordingLog {
        fn log(&self, message: &str) {
            self.lines.lock().unwrap().push(message.to_string());
        }

        fn soft_fail(&self, message: &str) {
            self.soft_fail_lines
                .lock()
                .unwrap()
                .push(message.to_string());
        }
    }

    /// Fake capture: reports format, sets energy, waits on stop.
    struct FakeCapture {
        sample_rate: u32,
        channels: u16,
        started: Arc<AtomicBool>,
        running: Arc<AtomicBool>,
    }

    impl CaptureBackend for FakeCapture {
        fn run(
            self: Box<Self>,
            _audio_tx: mpsc::Sender<Vec<f32>>,
            energy_out: Arc<AtomicU32>,
            stop: Arc<AtomicBool>,
            report_format: Box<dyn FnOnce(u32, u16) + Send>,
            log: Arc<dyn SessionLog>,
        ) {
            self.started.store(true, Ordering::SeqCst);
            self.running.store(true, Ordering::SeqCst);
            energy_out.store(0.25f32.to_bits(), Ordering::Relaxed);
            log.log(&format!(
                "capture started: {} Hz, {} ch — device: fake",
                self.sample_rate, self.channels
            ));
            report_format(self.sample_rate, self.channels);
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            self.running.store(false, Ordering::SeqCst);
            log.log("capture stopped");
        }
    }

    fn fake(
        started: &Arc<AtomicBool>,
        running: &Arc<AtomicBool>,
    ) -> Box<dyn CaptureBackend> {
        Box::new(FakeCapture {
            sample_rate: 48_000,
            channels: 1,
            started: started.clone(),
            running: running.clone(),
        })
    }

    // ── Seam: VoiceSession start/stop ────────────────────────────────────────

    #[test]
    fn start_runs_capture_and_reports_format() {
        let started = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(false));
        let energy = Arc::new(AtomicU32::new(0));
        let log = RecordingLog::new();

        // Lifecycle with STT absent still starts capture (soft-fail path).
        let (mut session, info) =
            VoiceSession::start(fake(&started, &running), energy.clone(), log, None)
                .expect("start");

        assert!(started.load(Ordering::SeqCst), "capture backend must run");
        assert!(running.load(Ordering::SeqCst));
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.channels, 1);
        assert_eq!(info.transcription, TranscriptionMode::Off);
        assert!(info.stt.is_none());
        assert!(f32::from_bits(energy.load(Ordering::Relaxed)) > 0.0);

        session.stop();
        assert!(!running.load(Ordering::SeqCst));
        assert_eq!(energy.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn stop_is_idempotent() {
        let started = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(false));
        let energy = Arc::new(AtomicU32::new(0));
        let log = RecordingLog::new();

        let (mut session, _) =
            VoiceSession::start(fake(&started, &running), energy, log, None).unwrap();

        session.stop();
        session.stop();
        session.stop();
        assert!(session.is_stopped());
    }

    #[test]
    fn drop_stops_capture() {
        let started = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(false));
        let energy = Arc::new(AtomicU32::new(0));
        let log = RecordingLog::new();

        {
            let (_session, _) =
                VoiceSession::start(fake(&started, &running), energy, log, None).unwrap();
            assert!(running.load(Ordering::SeqCst));
        }
        assert!(
            !running.load(Ordering::SeqCst),
            "Drop must tear down capture"
        );
    }

    #[test]
    fn soft_fail_starts_mic_with_transcription_off_and_repair_hint() {
        let started = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(false));
        let energy = Arc::new(AtomicU32::new(0));
        let log = RecordingLog::new();

        let (mut session, info) =
            VoiceSession::start(fake(&started, &running), energy, log.clone(), None)
                .unwrap();

        assert!(started.load(Ordering::SeqCst));
        assert_eq!(info.transcription, TranscriptionMode::Off);
        assert!(info.stt.is_none());
        let joined = log.soft_fail_joined();
        assert!(
            joined.contains("transcription off"),
            "soft-fail must say transcription is off: {joined}"
        );
        assert!(
            joined.contains("Setup Wizard") || joined.contains("Setup repair"),
            "soft-fail must point at Setup repair: {joined}"
        );

        session.stop();
    }
}

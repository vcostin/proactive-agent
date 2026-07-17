//! Deep Piper speak module — path, gate, resolve, synth/playback behind one interface.
//!
//! Callers pass text + voice id (or preview a voice id). They do not assemble
//! `tts/` paths, gate tokens, or vestigial client construction.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::piper_download::{download_curated_piper_voice, VoiceDownloadProgress, VoiceFileFetcher};
use super::piper_voice::{
    piper_voice_pair_present, resolve_piper_voice, PIPER_VOICE_PREVIEW_SAMPLE,
};
use super::tts::{PlaybackGate, PlaybackToken};

/// Debug / host diagnostics for speak — production may emit Tauri debug events.
pub trait SpeakLog: Send + Sync {
    fn log(&self, message: &str);
}

/// Emit `[AUDIO]` debug events via Tauri (sync — no AppHandle on the core speak path).
pub struct DebugEventSpeakLog {
    app: tauri::AppHandle,
}

impl DebugEventSpeakLog {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl SpeakLog for DebugEventSpeakLog {
    fn log(&self, message: &str) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "debug_event",
            crate::monitor::DebugEvent {
                timestamp: chrono::Utc::now(),
                component: "[AUDIO]".to_string(),
                message: message.to_string(),
            },
        );
    }
}

/// Turns cleaned text + an onnx model into audible output, honouring the gate.
#[async_trait]
pub trait SpeakEngine: Send + Sync {
    async fn speak_resolved(
        &self,
        text: &str,
        onnx_path: &Path,
        gate: Arc<PlaybackGate>,
        token: PlaybackToken,
    ) -> Result<()>;
}

/// Owns PlaybackGate and models → tts/ layout for speak and preview.
pub struct PiperSpeak<E, L> {
    models_dir: PathBuf,
    gate: Arc<PlaybackGate>,
    engine: E,
    log: L,
}

impl<E, L> PiperSpeak<E, L>
where
    E: SpeakEngine,
    L: SpeakLog,
{
    pub fn new(models_dir: PathBuf, engine: E, log: L) -> Self {
        Self {
            models_dir,
            gate: Arc::new(PlaybackGate::new()),
            engine,
            log,
        }
    }

    /// Shared gate for adapters that already hold a module handle (tests / supersede).
    pub fn gate(&self) -> Arc<PlaybackGate> {
        self.gate.clone()
    }

    fn tts_dir(&self) -> PathBuf {
        self.models_dir.join("tts")
    }

    /// Speak `text` for `voice_id` (resolve + fallback inside). Supersedes in-flight playback.
    pub async fn speak(&self, text: &str, voice_id: &str) -> Result<()> {
        let clean = clean_for_speech(text);
        if clean.trim().is_empty() {
            return Ok(());
        }

        let token = self.gate.begin();
        let tts_dir = self.tts_dir();
        let resolved = match resolve_piper_voice(&tts_dir, voice_id) {
            Ok(r) => {
                if r.id != voice_id {
                    self.log.log(&format!(
                        "TTS: voice '{voice_id}' unavailable — falling back to '{}'",
                        r.id
                    ));
                }
                r
            }
            Err(e) => {
                self.log.log(&format!("TTS: {e}"));
                return Err(anyhow::anyhow!("{e}"));
            }
        };

        self.log
            .log(&format!("TTS: {} chars → {}", clean.len(), resolved.id));
        match self
            .engine
            .speak_resolved(&clean, &resolved.onnx_path, self.gate.clone(), token)
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => {
                self.log.log(&format!("TTS failed: {e}"));
                Err(e)
            }
        }
    }

    /// Preview fixed sample for `voice_id` without mutating config.
    /// If the curated onnx+json pair is missing, downloads it first via `fetcher`.
    pub async fn preview<F, P>(
        &self,
        voice_id: &str,
        fetcher: &F,
        mut on_progress: P,
    ) -> Result<()>
    where
        F: VoiceFileFetcher,
        P: FnMut(VoiceDownloadProgress),
    {
        let tts_dir = self.tts_dir();
        if !piper_voice_pair_present(&tts_dir, voice_id) {
            download_curated_piper_voice(voice_id, &tts_dir, fetcher, &mut on_progress)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        self.speak(PIPER_VOICE_PREVIEW_SAMPLE, voice_id).await
    }
}

/// Strip markdown so it isn't read aloud as noise.
pub fn clean_for_speech(text: &str) -> String {
    let mut s = text.to_string();
    while let Some(start) = s.find("```") {
        if let Some(rel) = s[start + 3..].find("```") {
            s = format!("{}{}", &s[..start], &s[start + 3 + rel + 3..]);
        } else {
            break;
        }
    }
    s = s.replace('`', "");
    s = s.replace("**", "").replace("__", "").replace('*', "").replace('_', " ");
    if let Ok(re) = regex::Regex::new(r"https?://\S+") {
        s = re.replace_all(&s, "").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"(?s)<defer>.*?</defer>") {
        s = re.replace_all(&s, "").to_string();
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::audio::piper_download::VoiceFileFetcher;

    struct NullLog;
    impl SpeakLog for NullLog {
        fn log(&self, _message: &str) {}
    }

    #[derive(Default)]
    struct RecordingEngine {
        /// (text, onnx file name, played?)
        calls: Mutex<Vec<(String, String, bool)>>,
        /// When set, a held speak blocks until this is true (supersede tests).
        release: Mutex<Option<Arc<AtomicBool>>>,
        hold_while: Mutex<Option<Arc<AtomicBool>>>,
        /// How many speak_resolved calls should wait on `release` (usually 1).
        hold_remaining: AtomicUsize,
    }

    #[async_trait]
    impl SpeakEngine for RecordingEngine {
        async fn speak_resolved(
            &self,
            text: &str,
            onnx_path: &Path,
            gate: Arc<PlaybackGate>,
            token: PlaybackToken,
        ) -> Result<()> {
            let name = onnx_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();

            // Only the first N calls hold — later speaks must finish so supersede can complete.
            let should_hold = self
                .hold_remaining
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                    if n > 0 {
                        Some(n - 1)
                    } else {
                        None
                    }
                })
                .is_ok();

            if should_hold {
                if let Some(flag) = self.hold_while.lock().unwrap().as_ref() {
                    flag.store(true, Ordering::SeqCst);
                }
                let release = self.release.lock().unwrap().clone();
                if let Some(rel) = release {
                    while !rel.load(Ordering::SeqCst) {
                        if !gate.is_current(token) {
                            self.calls
                                .lock()
                                .unwrap()
                                .push((text.to_string(), name, false));
                            return Ok(());
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                }
            }

            let played = gate.is_current(token);
            self.calls
                .lock()
                .unwrap()
                .push((text.to_string(), name, played));
            Ok(())
        }
    }

    struct FixtureFetcher {
        /// Maps URL suffix / voice file → bytes to write.
        files: Mutex<Vec<(String, Vec<u8>)>>,
        fetch_count: AtomicUsize,
    }

    impl FixtureFetcher {
        fn for_voice(voice_id: &str) -> Self {
            Self {
                files: Mutex::new(vec![
                    (
                        format!("{voice_id}.onnx"),
                        format!("onnx-{voice_id}").into_bytes(),
                    ),
                    (
                        format!("{voice_id}.onnx.json"),
                        format!("json-{voice_id}").into_bytes(),
                    ),
                ]),
                fetch_count: AtomicUsize::new(0),
            }
        }
    }

    impl VoiceFileFetcher for FixtureFetcher {
        fn fetch_to(
            &self,
            url: &str,
            dest: &Path,
            on_bytes: &mut dyn FnMut(u64, u64),
        ) -> Result<(), String> {
            self.fetch_count.fetch_add(1, Ordering::SeqCst);
            let files = self.files.lock().unwrap();
            let bytes = files
                .iter()
                .find(|(name, _)| url.ends_with(name) || url.contains(name))
                .map(|(_, b)| b.clone())
                .ok_or_else(|| format!("fixture missing for {url}"))?;
            let total = bytes.len() as u64;
            on_bytes(0, total);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(dest, &bytes).map_err(|e| e.to_string())?;
            on_bytes(total, total);
            Ok(())
        }
    }

    fn unique_temp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pa-piper-speak-{label}-{nanos}"))
    }

    fn write_voice_pair(tts_dir: &Path, id: &str) {
        fs::create_dir_all(tts_dir).unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx")), b"onnx").unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx.json")), b"{}").unwrap();
    }

    // ── Seam: speak ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn speak_owns_resolve_and_plays_for_voice_id() {
        let root = unique_temp("speak");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-joe-medium");

        let engine = RecordingEngine::default();
        let speak = PiperSpeak::new(root.clone(), engine, NullLog);
        speak
            .speak("Hello there.", "en_US-joe-medium")
            .await
            .unwrap();

        let calls = speak.engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Hello there.");
        assert_eq!(calls[0].1, "en_US-joe-medium.onnx");
        assert!(calls[0].2, "current generation must play");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn speak_empty_after_clean_is_noop() {
        let root = unique_temp("empty");
        let engine = RecordingEngine::default();
        let speak = PiperSpeak::new(root.clone(), engine, NullLog);
        speak.speak("   ", "en_US-lessac-medium").await.unwrap();
        assert!(speak.engine.calls.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    // ── Seam: preview ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn preview_speaks_fixed_sample_without_config_handle() {
        let root = unique_temp("preview");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-joe-medium");

        let engine = RecordingEngine::default();
        let speak = PiperSpeak::new(root.clone(), engine, NullLog);
        let fetcher = FixtureFetcher::for_voice("en_US-joe-medium");
        speak
            .preview("en_US-joe-medium", &fetcher, |_| {})
            .await
            .unwrap();

        let calls = speak.engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, PIPER_VOICE_PREVIEW_SAMPLE);
        assert_eq!(calls[0].1, "en_US-joe-medium.onnx");
        assert_eq!(fetcher.fetch_count.load(Ordering::SeqCst), 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn preview_downloads_missing_curated_pair_then_speaks() {
        let root = unique_temp("ensure");
        fs::create_dir_all(root.join("tts")).unwrap();

        let engine = RecordingEngine::default();
        let speak = PiperSpeak::new(root.clone(), engine, NullLog);
        let fetcher = FixtureFetcher::for_voice("en_US-kristin-medium");
        speak
            .preview("en_US-kristin-medium", &fetcher, |_| {})
            .await
            .unwrap();

        assert!(
            fetcher.fetch_count.load(Ordering::SeqCst) >= 2,
            "must fetch onnx + json"
        );
        assert!(piper_voice_pair_present(
            &root.join("tts"),
            "en_US-kristin-medium"
        ));

        let calls = speak.engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, PIPER_VOICE_PREVIEW_SAMPLE);
        assert_eq!(calls[0].1, "en_US-kristin-medium.onnx");

        let _ = fs::remove_dir_all(&root);
    }

    // ── Seam: supersede ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn later_speak_supersedes_in_flight_playback() {
        let root = unique_temp("supersede");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-lessac-medium");

        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let engine = RecordingEngine {
            hold_while: Mutex::new(Some(started.clone())),
            release: Mutex::new(Some(release.clone())),
            hold_remaining: AtomicUsize::new(1),
            ..Default::default()
        };
        let speak = Arc::new(PiperSpeak::new(root.clone(), engine, NullLog));

        let s1 = speak.clone();
        let first = tokio::spawn(async move {
            s1.speak("first utterance", "en_US-lessac-medium").await
        });

        while !started.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        speak
            .speak("second utterance", "en_US-lessac-medium")
            .await
            .unwrap();
        release.store(true, Ordering::SeqCst);
        first.await.unwrap().unwrap();

        let calls = speak.engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let first_call = calls
            .iter()
            .find(|(t, _, _)| t == "first utterance")
            .expect("first speak recorded");
        let second_call = calls
            .iter()
            .find(|(t, _, _)| t == "second utterance")
            .expect("second speak recorded");
        assert!(
            !first_call.2,
            "first must be superseded (played=false)"
        );
        assert!(second_call.2, "second must play");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn speak_supersedes_in_flight_preview() {
        let root = unique_temp("preview-supersede");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-joe-medium");

        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let engine = RecordingEngine {
            hold_while: Mutex::new(Some(started.clone())),
            release: Mutex::new(Some(release.clone())),
            hold_remaining: AtomicUsize::new(1),
            ..Default::default()
        };
        let speak = Arc::new(PiperSpeak::new(root.clone(), engine, NullLog));
        let fetcher = FixtureFetcher::for_voice("en_US-joe-medium");

        let s1 = speak.clone();
        let first = tokio::spawn(async move {
            s1.preview("en_US-joe-medium", &fetcher, |_| {}).await
        });

        while !started.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        speak
            .speak("chat reply", "en_US-joe-medium")
            .await
            .unwrap();
        release.store(true, Ordering::SeqCst);
        first.await.unwrap().unwrap();

        let calls = speak.engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let preview_call = calls
            .iter()
            .find(|(t, _, _)| t == PIPER_VOICE_PREVIEW_SAMPLE)
            .expect("preview recorded");
        let speak_call = calls
            .iter()
            .find(|(t, _, _)| t == "chat reply")
            .expect("speak recorded");
        assert!(!preview_call.2, "preview must be superseded");
        assert!(speak_call.2, "speak must play");

        let _ = fs::remove_dir_all(&root);
    }
}

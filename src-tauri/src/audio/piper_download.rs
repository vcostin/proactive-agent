//! Download curated Piper voices (onnx + json) on demand.
//!
//! Fetches pinned Hugging Face `v1.0.0` URLs into `models/tts/`. A voice is
//! only marked installed when both files verify. Does not mutate voice
//! selection / config — callers own that.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::piper_catalog::{curated_voice_hf_stem, is_curated_piper_voice_id};
use super::piper_voice::piper_voice_pair_present;

/// Hugging Face resolve base for Piper voices tag `v1.0.0`.
pub const PIPER_VOICES_HF_V1: &str =
    "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";

/// Progress for one file while downloading a curated voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDownloadProgress {
    pub voice_id: String,
    pub filename: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
}

/// System boundary: stream a URL to a local path (tests inject fixtures; prod uses HTTP).
pub trait VoiceFileFetcher {
    /// Write `url` to `dest`, calling `on_bytes(downloaded, total)` as data arrives.
    /// `total` may be 0 when the server omits Content-Length.
    fn fetch_to(
        &self,
        url: &str,
        dest: &Path,
        on_bytes: &mut dyn FnMut(u64, u64),
    ) -> Result<(), String>;
}

/// Production fetcher — blocking HTTP GET via reqwest, streamed to disk.
pub struct HttpVoiceFileFetcher {
    client: reqwest::blocking::Client,
}

impl HttpVoiceFileFetcher {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("proactive-agent/1.0")
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Self { client })
    }
}

impl VoiceFileFetcher for HttpVoiceFileFetcher {
    fn fetch_to(
        &self,
        url: &str,
        dest: &Path,
        on_bytes: &mut dyn FnMut(u64, u64),
    ) -> Result<(), String> {
        let mut resp = self
            .client
            .get(url)
            .send()
            .map_err(|e| format!("download {url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("download {url}: HTTP {}", resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let mut file = File::create(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
        let mut buf = [0u8; 64 * 1024];
        let mut downloaded = 0u64;
        on_bytes(0, total);
        loop {
            let n = resp
                .read(&mut buf)
                .map_err(|e| format!("download {url}: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("write {}: {e}", dest.display()))?;
            downloaded += n as u64;
            on_bytes(downloaded, total);
        }
        file.sync_all()
            .map_err(|e| format!("sync {}: {e}", dest.display()))?;
        Ok(())
    }
}

/// Download both `{id}.onnx` and `{id}.onnx.json` for a curated voice id.
///
/// On success both files are present under `tts_dir` and the catalog reports
/// the voice installed. On failure no incomplete pair is left installed;
/// prior selection is untouched (this function never reads/writes config).
pub fn download_curated_piper_voice<F, P>(
    voice_id: &str,
    tts_dir: &Path,
    fetcher: &F,
    mut on_progress: P,
) -> Result<(), String>
where
    F: VoiceFileFetcher,
    P: FnMut(VoiceDownloadProgress),
{
    if !is_curated_piper_voice_id(voice_id) {
        return Err(format!("unknown curated piper voice id: {voice_id}"));
    }
    if piper_voice_pair_present(tts_dir, voice_id) {
        emit_already_present(voice_id, &mut on_progress);
        return Ok(());
    }

    let (onnx_url, json_url) = curated_voice_download_urls(voice_id)?;
    fs::create_dir_all(tts_dir).map_err(|e| format!("create tts dir: {e}"))?;

    let onnx_name = format!("{voice_id}.onnx");
    let json_name = format!("{voice_id}.onnx.json");
    let onnx_partial = partial_path(tts_dir, &onnx_name);
    let json_partial = partial_path(tts_dir, &json_name);
    let onnx_dest = tts_dir.join(&onnx_name);
    let json_dest = tts_dir.join(&json_name);

    // Clean stale partials from a previous failed attempt.
    let _ = fs::remove_file(&onnx_partial);
    let _ = fs::remove_file(&json_partial);

    let result = (|| {
        fetch_file_with_progress(
            voice_id,
            &onnx_name,
            &onnx_url,
            &onnx_partial,
            fetcher,
            &mut on_progress,
        )?;
        fetch_file_with_progress(
            voice_id,
            &json_name,
            &json_url,
            &json_partial,
            fetcher,
            &mut on_progress,
        )?;

        // Promote only when both partials exist — avoids onnx-only "installed".
        fs::rename(&onnx_partial, &onnx_dest).map_err(|e| format!("promote {onnx_name}: {e}"))?;
        fs::rename(&json_partial, &json_dest).map_err(|e| {
            let _ = fs::remove_file(&onnx_dest);
            format!("promote {json_name}: {e}")
        })?;

        if !piper_voice_pair_present(tts_dir, voice_id) {
            let _ = fs::remove_file(&onnx_dest);
            let _ = fs::remove_file(&json_dest);
            return Err(format!(
                "download of '{voice_id}' did not leave a verified onnx+json pair"
            ));
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&onnx_partial);
        let _ = fs::remove_file(&json_partial);
        // If we promoted onnx but failed later, remove the orphan.
        if onnx_dest.is_file() && !json_dest.is_file() {
            let _ = fs::remove_file(&onnx_dest);
        }
    }

    result
}

/// Absolute HF URLs for a curated voice’s onnx + json (pinned `v1.0.0`).
pub fn curated_voice_download_urls(voice_id: &str) -> Result<(String, String), String> {
    let stem = curated_voice_hf_stem(voice_id)
        .ok_or_else(|| format!("unknown curated piper voice id: {voice_id}"))?;
    let onnx = format!("{PIPER_VOICES_HF_V1}/{stem}.onnx");
    let json = format!("{PIPER_VOICES_HF_V1}/{stem}.onnx.json");
    Ok((onnx, json))
}

fn partial_path(tts_dir: &Path, filename: &str) -> PathBuf {
    tts_dir.join(format!("{filename}.download-partial"))
}

fn fetch_file_with_progress<F, P>(
    voice_id: &str,
    filename: &str,
    url: &str,
    dest: &Path,
    fetcher: &F,
    on_progress: &mut P,
) -> Result<(), String>
where
    F: VoiceFileFetcher,
    P: FnMut(VoiceDownloadProgress),
{
    let mut report = |downloaded: u64, total: u64, done: bool| {
        on_progress(VoiceDownloadProgress {
            voice_id: voice_id.to_string(),
            filename: filename.to_string(),
            downloaded,
            total,
            done,
        });
    };
    report(0, 0, false);
    let mut last = (0u64, 0u64);
    fetcher.fetch_to(url, dest, &mut |downloaded, total| {
        last = (downloaded, total);
        report(downloaded, total, false);
    })?;
    report(last.0, last.1, true);
    Ok(())
}

fn emit_already_present<P>(voice_id: &str, on_progress: &mut P)
where
    P: FnMut(VoiceDownloadProgress),
{
    for filename in [
        format!("{voice_id}.onnx"),
        format!("{voice_id}.onnx.json"),
    ] {
        on_progress(VoiceDownloadProgress {
            voice_id: voice_id.to_string(),
            filename,
            downloaded: 0,
            total: 0,
            done: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::piper_catalog::list_curated_piper_voices;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pa-piper-dl-{label}-{nanos}"))
    }

    struct MapFetcher {
        by_url: HashMap<String, Result<Vec<u8>, String>>,
        calls: RefCell<Vec<String>>,
    }

    impl VoiceFileFetcher for MapFetcher {
        fn fetch_to(
            &self,
            url: &str,
            dest: &Path,
            on_bytes: &mut dyn FnMut(u64, u64),
        ) -> Result<(), String> {
            self.calls.borrow_mut().push(url.to_string());
            match self.by_url.get(url) {
                Some(Ok(bytes)) => {
                    let total = bytes.len() as u64;
                    // Simulate chunked progress for the busy/progress seam.
                    on_bytes(0, total);
                    if total > 0 {
                        on_bytes(total, total);
                    }
                    fs::write(dest, bytes).map_err(|e| format!("write fixture: {e}"))
                }
                Some(Err(e)) => Err(e.clone()),
                None => Err(format!("unexpected url: {url}")),
            }
        }
    }

    fn entry_installed(tts_dir: &Path, id: &str) -> bool {
        list_curated_piper_voices(tts_dir)
            .into_iter()
            .find(|v| v.id == id)
            .map(|v| v.installed)
            .unwrap_or(false)
    }

    #[test]
    fn download_fetches_onnx_and_json_and_marks_installed() {
        let root = unique_temp("ok");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();

        let id = "en_US-joe-medium";
        let (onnx_url, json_url) = curated_voice_download_urls(id).unwrap();
        assert_eq!(
            onnx_url,
            "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/joe/medium/en_US-joe-medium.onnx"
        );
        assert_eq!(
            json_url,
            "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/joe/medium/en_US-joe-medium.onnx.json"
        );

        let fetcher = MapFetcher {
            by_url: HashMap::from([
                (onnx_url.clone(), Ok(b"onnx-bytes".to_vec())),
                (json_url.clone(), Ok(b"{\"audio\":{}}".to_vec())),
            ]),
            calls: RefCell::new(Vec::new()),
        };

        let progress = RefCell::new(Vec::new());
        download_curated_piper_voice(id, &tts, &fetcher, |p| {
            progress.borrow_mut().push(p);
        })
        .expect("download should succeed");

        assert!(entry_installed(&tts, id), "catalog must report installed");
        assert!(piper_voice_pair_present(&tts, id));
        assert_eq!(
            fs::read(tts.join("en_US-joe-medium.onnx")).unwrap(),
            b"onnx-bytes"
        );
        assert_eq!(
            fs::read(tts.join("en_US-joe-medium.onnx.json")).unwrap(),
            b"{\"audio\":{}}"
        );

        let calls = fetcher.calls.borrow().clone();
        assert_eq!(calls, vec![onnx_url, json_url]);

        let events = progress.borrow().clone();
        assert!(
            events
                .iter()
                .any(|e| !e.done && e.filename == "en_US-joe-medium.onnx"),
            "expected in-progress event for onnx: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| e.done && e.filename == "en_US-joe-medium.onnx"),
            "expected a done progress event for onnx: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| e.done && e.filename == "en_US-joe-medium.onnx.json"),
            "expected a done progress event for json: {events:?}"
        );
        assert!(
            events.iter().all(|e| e.voice_id == id),
            "progress must carry voice id"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_json_fetch_does_not_mark_installed_or_leave_onnx_only() {
        let root = unique_temp("fail-json");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();

        let id = "en_US-kristin-medium";
        let (onnx_url, json_url) = curated_voice_download_urls(id).unwrap();
        let fetcher = MapFetcher {
            by_url: HashMap::from([
                (onnx_url, Ok(b"onnx".to_vec())),
                (json_url, Err("network down".into())),
            ]),
            calls: RefCell::new(Vec::new()),
        };

        let err = download_curated_piper_voice(id, &tts, &fetcher, |_| {})
            .expect_err("json failure should error");
        assert!(
            err.contains("network down"),
            "error should surface failure: {err}"
        );

        assert!(!entry_installed(&tts, id));
        assert!(!tts.join(format!("{id}.onnx")).is_file());
        assert!(!tts.join(format!("{id}.onnx.json")).is_file());
        assert!(
            !tts.join(format!("{id}.onnx.download-partial")).is_file(),
            "partials must be cleaned up"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_voice_id_is_rejected() {
        let root = unique_temp("unknown");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();

        let fetcher = MapFetcher {
            by_url: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        };
        let err =
            download_curated_piper_voice("en_US-amy-medium", &tts, &fetcher, |_| {})
                .expect_err("non-curated id must fail");
        assert!(err.contains("unknown"), "{err}");
        assert!(fetcher.calls.borrow().is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn platform_wizard_catalog_stays_default_voice_only() {
        // Curated extras are download-on-select from Settings — not wizard steps.
        let curated_extras = [
            "en_US-joe-medium",
            "en_US-kristin-medium",
            "en_US-bryce-medium",
            "en_US-sam-medium",
            "en_GB-cori-medium",
        ];
        for module in crate::platform::all_modules() {
            for def in module.artifacts() {
                for extra in curated_extras {
                    assert!(
                        !def.id.contains(extra) && !def.filename.contains(extra),
                        "{} artifact {} / {} must not be a curated extra wizard step",
                        module.id().as_str(),
                        def.id,
                        def.filename
                    );
                }
            }
            // If a default TTS voice artifact exists, it must be lessac only.
            for def in module.artifacts().iter().filter(|d| d.id == "tts-voice") {
                assert_eq!(
                    def.filename,
                    crate::constants::TTS_MODEL_FILE,
                    "wizard TTS voice must remain the default lessac onnx"
                );
            }
        }
    }
}

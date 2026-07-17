//! Resolve which Piper voice files to use for speak/preview.
//!
//! A voice is usable only when both `{id}.onnx` and `{id}.onnx.json` exist
//! under the models/tts directory. Missing or incomplete selections fall
//! back to the default voice id.

use std::path::{Path, PathBuf};

/// Stable Piper voice id used when nothing else is selected or usable.
pub use crate::constants::TTS_DEFAULT_VOICE_ID as DEFAULT_PIPER_VOICE_ID;

/// Fixed sample spoken by Voice preview (same copy as the debug TTS button).
pub const PIPER_VOICE_PREVIEW_SAMPLE: &str =
    "Hello, I am your proactive assistant. Voice synthesis is working.";

/// Paths Piper needs for one resolved voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPiperVoice {
    pub id: String,
    pub onnx_path: PathBuf,
}

/// Speak payload for preview — target voice id + fixed sample, never config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiperVoicePreviewRequest {
    pub text: String,
    pub voice_id: String,
}

/// Build a preview request for `voice_id` using the product fixed sample.
/// Does not read or write the persisted selected voice.
pub fn preview_piper_voice_request(voice_id: &str) -> PiperVoicePreviewRequest {
    PiperVoicePreviewRequest {
        text: PIPER_VOICE_PREVIEW_SAMPLE.to_string(),
        voice_id: voice_id.to_string(),
    }
}

/// Resolve `selected_id` under `tts_dir`, falling back to
/// [`DEFAULT_PIPER_VOICE_ID`] when the selection is missing or incomplete.
pub fn resolve_piper_voice(
    tts_dir: &Path,
    selected_id: &str,
) -> Result<ResolvedPiperVoice, String> {
    if let Some(resolved) = try_resolve(tts_dir, selected_id) {
        return Ok(resolved);
    }
    if selected_id != DEFAULT_PIPER_VOICE_ID {
        if let Some(resolved) = try_resolve(tts_dir, DEFAULT_PIPER_VOICE_ID) {
            return Ok(resolved);
        }
    }
    Err(format!(
        "piper voice not found — tried '{selected_id}' and default '{DEFAULT_PIPER_VOICE_ID}' (need .onnx + .onnx.json)"
    ))
}

/// Whether both `{id}.onnx` and `{id}.onnx.json` exist under `tts_dir`.
pub fn piper_voice_pair_present(tts_dir: &Path, id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let onnx_path = tts_dir.join(format!("{id}.onnx"));
    let json_path = tts_dir.join(format!("{id}.onnx.json"));
    onnx_path.is_file() && json_path.is_file()
}

fn try_resolve(tts_dir: &Path, id: &str) -> Option<ResolvedPiperVoice> {
    if !piper_voice_pair_present(tts_dir, id) {
        return None;
    }
    Some(ResolvedPiperVoice {
        id: id.to_string(),
        onnx_path: tts_dir.join(format!("{id}.onnx")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pa-piper-voice-{label}-{nanos}"))
    }

    fn write_voice_pair(tts_dir: &Path, id: &str) {
        fs::create_dir_all(tts_dir).unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx")), b"onnx").unwrap();
        fs::write(tts_dir.join(format!("{id}.onnx.json")), b"{}").unwrap();
    }

    #[test]
    fn preview_request_uses_fixed_sample_and_target_voice() {
        let req = preview_piper_voice_request("en_US-joe-medium");
        assert_eq!(
            req.text,
            "Hello, I am your proactive assistant. Voice synthesis is working."
        );
        assert_eq!(req.voice_id, "en_US-joe-medium");
    }

    #[test]
    fn preview_request_does_not_depend_on_default_voice_id() {
        let req = preview_piper_voice_request("en_GB-cori-medium");
        assert_ne!(req.voice_id, DEFAULT_PIPER_VOICE_ID);
        assert_eq!(req.voice_id, "en_GB-cori-medium");
        assert_eq!(req.text, PIPER_VOICE_PREVIEW_SAMPLE);
    }

    #[test]
    fn resolves_selected_voice_when_onnx_and_json_present() {
        let root = unique_temp("selected");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-joe-medium");

        let resolved = resolve_piper_voice(&tts, "en_US-joe-medium").unwrap();
        assert_eq!(resolved.id, "en_US-joe-medium");
        assert_eq!(resolved.onnx_path, tts.join("en_US-joe-medium.onnx"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn falls_back_to_default_when_selection_missing() {
        let root = unique_temp("fallback");
        let tts = root.join("tts");
        write_voice_pair(&tts, "en_US-lessac-medium");

        let resolved = resolve_piper_voice(&tts, "en_US-joe-medium").unwrap();
        assert_eq!(resolved.id, "en_US-lessac-medium");
        assert_eq!(resolved.onnx_path, tts.join("en_US-lessac-medium.onnx"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn onnx_only_selection_falls_back_to_default_pair() {
        let root = unique_temp("onnx-only");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();
        fs::write(tts.join("en_US-joe-medium.onnx"), b"onnx").unwrap();
        write_voice_pair(&tts, "en_US-lessac-medium");

        let resolved = resolve_piper_voice(&tts, "en_US-joe-medium").unwrap();
        assert_eq!(resolved.id, "en_US-lessac-medium");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn default_onnx_only_without_json_is_unresolved() {
        let root = unique_temp("default-onnx-only");
        let tts = root.join("tts");
        fs::create_dir_all(&tts).unwrap();
        fs::write(tts.join("en_US-lessac-medium.onnx"), b"onnx").unwrap();

        let err = resolve_piper_voice(&tts, "en_US-lessac-medium").unwrap_err();
        assert!(
            err.contains("en_US-lessac-medium"),
            "error should mention the voice id: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }
}

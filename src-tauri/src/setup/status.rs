//! Setup status evaluation at the Rust command seam.
//!
//! Core agent readiness = chat model present + llama-server ready.
//! Piper/TTS is reported but never required for main UI.
//! Host STT readiness = STT model file + parakeet launcher, reported separately.

use std::path::{Path, PathBuf};

use crate::binary_store::BinariesStatus;
use crate::constants;
use crate::sidecar_filename;

/// Fixture-friendly snapshot of what the setup seam observes on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupProbe {
    pub chat_model_present: bool,
    pub llama_ready: bool,
    pub piper_ready: bool,
    pub parakeet_ready: bool,
    pub embed_model_ready: bool,
    pub stt_model_ready: bool,
}

/// Core agent can reach the main UI: chat model + inference sidecar.
/// Piper/TTS is intentionally excluded from this Host completion bar.
pub fn core_agent_ready(probe: &SetupProbe) -> bool {
    probe.chat_model_present && probe.llama_ready
}

/// Host STT path (mic → text) app-managed pieces are both present.
pub fn host_stt_ready(probe: &SetupProbe) -> bool {
    probe.stt_model_ready && probe.parakeet_ready
}

/// Probe sidecar readiness under an arbitrary binaries root (fixture tests).
pub fn check_binaries_in(binaries_root: &Path) -> BinariesStatus {
    BinariesStatus {
        llama_ready: find_sidecar_in(binaries_root, "llama-server").is_some(),
        piper_ready: find_sidecar_in(binaries_root, "piper").is_some(),
        parakeet_ready: find_sidecar_in(binaries_root, "parakeet-server").is_some(),
        parakeet_note: parakeet_note(),
    }
}

fn parakeet_note() -> String {
    #[cfg(target_os = "linux")]
    {
        "Host STT path: managed launcher under binaries/parakeet/ (repair via Setup Wizard)."
            .into()
    }
    #[cfg(not(target_os = "linux"))]
    {
        "Speech-to-text requires a Platform-module / manual install. Open Setup repair when ready."
            .into()
    }
}

/// Locate a sidecar under `binaries_root` using the same layout rules as production.
pub fn find_sidecar_in(binaries_root: &Path, name: &str) -> Option<PathBuf> {
    let filename = sidecar_filename(name);
    let short = name.split('-').next().unwrap_or(name);
    let candidates = [
        binaries_root.join(short).join(&filename),
        binaries_root.join(name).join(&filename),
        binaries_root.join(&filename),
    ];
    candidates
        .into_iter()
        .find(|p| crate::sidecar_file_usable(p))
}

/// Whether the STT ONNX model file exists under a binaries root.
pub fn stt_model_ready_in(binaries_root: &Path) -> bool {
    binaries_root
        .join("parakeet")
        .join("models")
        .join(constants::STT_MODEL_FILE)
        .exists()
}

/// Whether the embed model file exists under a models root.
pub fn embed_model_ready_in(models_dir: &Path) -> bool {
    models_dir.join(constants::EMBED_MODEL_FILE).exists()
}

/// Build the full setup probe from fixture (or live) directories.
pub fn probe_layout(
    binaries_root: &Path,
    models_dir: &Path,
    chat_model: &Path,
) -> SetupProbe {
    let binaries = check_binaries_in(binaries_root);
    SetupProbe {
        chat_model_present: !chat_model.as_os_str().is_empty() && chat_model.exists(),
        llama_ready: binaries.llama_ready,
        piper_ready: binaries.piper_ready,
        parakeet_ready: binaries.parakeet_ready,
        embed_model_ready: embed_model_ready_in(models_dir),
        stt_model_ready: stt_model_ready_in(binaries_root),
    }
}

/// Serialize-ready status payload shared with the frontend / Setup Wizard.
#[derive(serde::Serialize, Clone)]
pub struct SetupStatus {
    /// Core agent ready for main UI (chat model + llama). Not gated on Piper.
    pub ready: bool,
    pub chat_model: String,
    pub embed_model_ready: bool,
    /// Parakeet TDT ONNX model file present under binaries/parakeet/models/
    pub stt_model_ready: bool,
    /// Host STT path ready: ONNX model + parakeet launcher both present.
    pub stt_ready: bool,
    pub data_dir: String,
    pub binaries: BinariesStatus,
}

pub fn build_setup_status(
    chat_model: &str,
    models_dir: &Path,
    binaries_root: &Path,
    data_dir: String,
) -> SetupStatus {
    let chat_path = Path::new(chat_model);
    let probe = probe_layout(binaries_root, models_dir, chat_path);
    let binaries = check_binaries_in(binaries_root);
    SetupStatus {
        ready: core_agent_ready(&probe),
        chat_model: chat_model.to_string(),
        embed_model_ready: probe.embed_model_ready,
        stt_model_ready: probe.stt_model_ready,
        stt_ready: host_stt_ready(&probe),
        data_dir,
        binaries,
    }
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
        std::env::temp_dir().join(format!("pa-setup-{label}-{nanos}"))
    }

    fn write_sidecar(root: &Path, name: &str) {
        let filename = sidecar_filename(name);
        let short = name.split('-').next().unwrap_or(name);
        let dir = root.join(short);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(&filename);
        // Unix usability gate: >32 bytes + executable bit
        fs::write(&path, vec![0u8; 64]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
    }

    fn write_file(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn core_agent_ready_requires_chat_model_and_llama_not_piper() {
        let base = SetupProbe {
            chat_model_present: true,
            llama_ready: true,
            piper_ready: false,
            parakeet_ready: false,
            embed_model_ready: false,
            stt_model_ready: false,
        };
        assert!(core_agent_ready(&base));
        assert!(!core_agent_ready(&SetupProbe {
            chat_model_present: false,
            ..base.clone()
        }));
        assert!(!core_agent_ready(&SetupProbe {
            llama_ready: false,
            ..base
        }));
    }

    #[test]
    fn host_stt_ready_requires_model_and_launcher() {
        let base = SetupProbe {
            chat_model_present: true,
            llama_ready: true,
            piper_ready: true,
            parakeet_ready: true,
            embed_model_ready: true,
            stt_model_ready: true,
        };
        assert!(host_stt_ready(&base));
        assert!(!host_stt_ready(&SetupProbe {
            stt_model_ready: false,
            ..base.clone()
        }));
        assert!(!host_stt_ready(&SetupProbe {
            parakeet_ready: false,
            ..base
        }));
    }

    #[test]
    fn fixture_status_allows_main_ui_without_piper() {
        let root = unique_temp("gate");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);

        let status = build_setup_status(
            chat.to_str().unwrap(),
            &models,
            &binaries,
            root.to_string_lossy().into(),
        );

        assert!(status.ready, "Core agent ready without piper");
        assert!(status.binaries.llama_ready);
        assert!(!status.binaries.piper_ready);
        assert!(!status.stt_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fixture_status_reports_absent_chat_model_as_not_ready() {
        let root = unique_temp("no-chat");
        let binaries = root.join("binaries");
        let models = root.join("models");
        write_sidecar(&binaries, "llama-server");

        let status = build_setup_status("", &models, &binaries, root.to_string_lossy().into());
        assert!(!status.ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fixture_status_reports_absent_llama_as_not_ready() {
        let root = unique_temp("no-llama");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_file(&chat, 128);
        fs::create_dir_all(&binaries).unwrap();

        let status = build_setup_status(
            chat.to_str().unwrap(),
            &models,
            &binaries,
            root.to_string_lossy().into(),
        );
        assert!(!status.ready);
        assert!(!status.binaries.llama_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fixture_status_surfaces_stt_distinctly_from_core() {
        let root = unique_temp("stt");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);
        write_sidecar(&binaries, "parakeet-server");
        write_file(
            &binaries
                .join("parakeet")
                .join("models")
                .join(constants::STT_MODEL_FILE),
            128,
        );

        let status = build_setup_status(
            chat.to_str().unwrap(),
            &models,
            &binaries,
            root.to_string_lossy().into(),
        );

        assert!(status.ready);
        assert!(status.stt_model_ready);
        assert!(status.binaries.parakeet_ready);
        assert!(status.stt_ready);
        // Piper still optional for Core agent
        assert!(!status.binaries.piper_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fixture_reports_piper_when_present_without_changing_gate() {
        let root = unique_temp("piper");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_sidecar(&binaries, "piper");
        write_file(&chat, 128);

        let status = build_setup_status(
            chat.to_str().unwrap(),
            &models,
            &binaries,
            root.to_string_lossy().into(),
        );
        assert!(status.ready);
        assert!(status.binaries.piper_ready);

        let _ = fs::remove_dir_all(&root);
    }
}

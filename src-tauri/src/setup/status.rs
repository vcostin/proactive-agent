//! Setup status evaluation at the Rust command seam.
//!
//! Core / Host STT readiness derives from Platform `verify_catalog` plus
//! `required_for_core` / `required_for_stt`. Chat model presence remains part of
//! Core ready. Piper/TTS is reported but never required for the Host completion bar.

use std::path::{Path, PathBuf};

use crate::binary_store::BinariesStatus;
use crate::constants;
use crate::platform::{verify_catalog, ArtifactDef, LayoutRoots, VerifyStatus};
use crate::sidecar_filename;

fn ort_note() -> String {
    "Host STT path: app-managed ONNX Runtime library under binaries/ort/ (repair via Setup Wizard)."
        .into()
}

fn ready_by_id(statuses: &[VerifyStatus], id: &str) -> bool {
    statuses
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.ready)
        .unwrap_or(false)
}

fn required_all_ready(
    artifacts: &[ArtifactDef],
    statuses: &[VerifyStatus],
    required: impl Fn(&ArtifactDef) -> bool,
) -> bool {
    artifacts
        .iter()
        .filter(|a| required(a))
        .all(|a| ready_by_id(statuses, a.id))
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

fn stt_models_dir(binaries_root: &Path) -> PathBuf {
    binaries_root.join(constants::STT_MODEL_REL_DIR)
}

/// Whether encoder + decoder ONNX files exist under a binaries root.
pub fn stt_model_ready_in(binaries_root: &Path) -> bool {
    let dir = stt_models_dir(binaries_root);
    dir.join(constants::STT_ENCODER_FILE).is_file()
        && dir.join(constants::STT_DECODER_FILE).is_file()
}

/// Whether the STT vocabulary file exists under a binaries root.
pub fn stt_vocab_ready_in(binaries_root: &Path) -> bool {
    stt_models_dir(binaries_root)
        .join(constants::STT_VOCAB_FILE)
        .is_file()
}

/// Whether an ONNX Runtime shared library is present under binaries/ort/.
pub fn ort_lib_ready_in(binaries_root: &Path) -> bool {
    let dir = binaries_root.join(constants::ORT_LIB_REL_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return false;
    };
    rd.filter_map(Result::ok).any(|e| {
        let name = e.file_name();
        let s = name.to_string_lossy();
        e.file_type().map(|t| t.is_file()).unwrap_or(false)
            && (s.contains(".so") || s.ends_with(".dll") || s.ends_with(".dylib"))
    })
}

/// Whether the embed model file exists under a models root.
pub fn embed_model_ready_in(models_dir: &Path) -> bool {
    models_dir.join(constants::EMBED_MODEL_FILE).exists()
}

fn binaries_from_verify(statuses: &[VerifyStatus]) -> BinariesStatus {
    BinariesStatus {
        llama_ready: ready_by_id(statuses, "llama-server"),
        piper_ready: ready_by_id(statuses, "piper"),
        ort_ready: ready_by_id(statuses, "onnxruntime"),
        ort_note: ort_note(),
    }
}

/// Probe sidecar / ORT readiness under an arbitrary binaries root via catalog verify.
pub fn check_binaries_in(binaries_root: &Path) -> BinariesStatus {
    let roots = LayoutRoots {
        binaries: binaries_root.to_path_buf(),
        models: PathBuf::new(),
    };
    let statuses = verify_catalog(crate::platform::current_module().artifacts(), &roots);
    binaries_from_verify(&statuses)
}

/// Serialize-ready status payload shared with the frontend / Setup Wizard.
#[derive(serde::Serialize, Clone)]
pub struct SetupStatus {
    /// Core agent ready for main UI (chat model + required_for_core catalog). Not gated on Piper.
    pub ready: bool,
    pub chat_model: String,
    pub embed_model_ready: bool,
    /// Encoder + decoder ONNX present under binaries/parakeet/models/.
    pub stt_model_ready: bool,
    /// Vocabulary file present.
    pub stt_vocab_ready: bool,
    /// Host STT path ready: all required_for_stt catalog artifacts verify.
    pub stt_ready: bool,
    pub data_dir: String,
    pub binaries: BinariesStatus,
}

/// Derive SetupStatus from Platform catalog verify + required_for_* (+ chat model for Core).
///
/// This is the Platform readiness seam: same layout roots + catalog → one ready answer.
pub fn derive_setup_status(
    artifacts: &[ArtifactDef],
    roots: &LayoutRoots,
    chat_model: &str,
    data_dir: String,
) -> SetupStatus {
    let chat_path = Path::new(chat_model);
    let chat_model_present = !chat_model.is_empty() && chat_path.exists();
    let statuses = verify_catalog(artifacts, roots);

    let core_artifacts_ready =
        required_all_ready(artifacts, &statuses, |a| a.required_for_core);
    let stt_ready = required_all_ready(artifacts, &statuses, |a| a.required_for_stt);

    let encoder = ready_by_id(&statuses, "stt-encoder");
    let decoder = ready_by_id(&statuses, "stt-decoder");

    SetupStatus {
        ready: chat_model_present && core_artifacts_ready,
        chat_model: chat_model.to_string(),
        embed_model_ready: ready_by_id(&statuses, "embed-model"),
        stt_model_ready: encoder && decoder,
        stt_vocab_ready: ready_by_id(&statuses, "stt-vocab"),
        stt_ready,
        data_dir,
        binaries: binaries_from_verify(&statuses),
    }
}

/// Production adapter: current Platform catalog + layout roots → SetupStatus.
pub fn build_setup_status(
    chat_model: &str,
    models_dir: &Path,
    binaries_root: &Path,
    data_dir: String,
) -> SetupStatus {
    derive_setup_status(
        crate::platform::current_module().artifacts(),
        &LayoutRoots {
            binaries: binaries_root.to_path_buf(),
            models: models_dir.to_path_buf(),
        },
        chat_model,
        data_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        linux, macos, windows, ArtifactKind, ArtifactRoot, ArtifactSource, VerifyRule,
    };
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

    fn write_stt_artifacts(binaries: &Path) {
        let models = binaries.join(constants::STT_MODEL_REL_DIR);
        write_file(&models.join(constants::STT_ENCODER_FILE), 128);
        write_file(&models.join(constants::STT_DECODER_FILE), 128);
        write_file(&models.join(constants::STT_VOCAB_FILE), 64);
        write_file(
            &binaries
                .join(constants::ORT_LIB_REL_DIR)
                .join(constants::ORT_LIB_FILENAME),
            64,
        );
    }

    /// Minimal catalog whose required flags differ from Host naming — proves gates follow flags.
    fn flag_driven_catalog() -> Vec<ArtifactDef> {
        vec![
            ArtifactDef {
                id: "core-bin",
                kind: ArtifactKind::Sidecar,
                root: ArtifactRoot::Binaries,
                relative_dir: "",
                filename: "llama-server",
                sidecar_name: true,
                source: ArtifactSource::Manual,
                verify: VerifyRule::SidecarUsable,
                required_for_core: true,
                required_for_stt: false,
            },
            ArtifactDef {
                id: "optional-tts",
                kind: ArtifactKind::Sidecar,
                root: ArtifactRoot::Binaries,
                relative_dir: "",
                filename: "piper",
                sidecar_name: true,
                source: ArtifactSource::Manual,
                verify: VerifyRule::SidecarUsable,
                required_for_core: false,
                required_for_stt: false,
            },
            ArtifactDef {
                id: "stt-piece-a",
                kind: ArtifactKind::Data,
                root: ArtifactRoot::Binaries,
                relative_dir: "flag-stt",
                filename: "a.bin",
                sidecar_name: false,
                source: ArtifactSource::Manual,
                verify: VerifyRule::Exists,
                required_for_core: false,
                required_for_stt: true,
            },
            ArtifactDef {
                id: "stt-piece-b",
                kind: ArtifactKind::Data,
                root: ArtifactRoot::Binaries,
                relative_dir: "flag-stt",
                filename: "b.bin",
                sidecar_name: false,
                source: ArtifactSource::Manual,
                verify: VerifyRule::Exists,
                required_for_core: false,
                required_for_stt: true,
            },
        ]
    }

    #[test]
    fn derived_gates_follow_required_for_flags_not_optional_rows() {
        let root = unique_temp("flags");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);
        write_file(&binaries.join("flag-stt").join("a.bin"), 8);
        write_file(&binaries.join("flag-stt").join("b.bin"), 8);
        // optional-tts deliberately missing

        let catalog = flag_driven_catalog();
        let roots = LayoutRoots {
            binaries: binaries.clone(),
            models: models.clone(),
        };
        let status = derive_setup_status(
            &catalog,
            &roots,
            chat.to_str().unwrap(),
            root.to_string_lossy().into(),
        );

        assert!(status.ready, "core-bin + chat; optional-tts must not gate Core");
        assert!(status.stt_ready, "both required_for_stt pieces present");

        fs::remove_file(binaries.join("flag-stt").join("b.bin")).unwrap();
        let status = derive_setup_status(
            &catalog,
            &roots,
            chat.to_str().unwrap(),
            root.to_string_lossy().into(),
        );
        assert!(status.ready);
        assert!(
            !status.stt_ready,
            "removing one required_for_stt artifact must clear Host STT ready"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn derived_core_requires_chat_model_even_when_catalog_core_ready() {
        let root = unique_temp("no-chat");
        let binaries = root.join("binaries");
        let models = root.join("models");
        write_sidecar(&binaries, "llama-server");
        write_file(&binaries.join("flag-stt").join("a.bin"), 8);
        write_file(&binaries.join("flag-stt").join("b.bin"), 8);

        let status = derive_setup_status(
            &flag_driven_catalog(),
            &LayoutRoots {
                binaries,
                models: models.clone(),
            },
            "",
            root.to_string_lossy().into(),
        );
        assert!(!status.ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn host_catalog_stt_ready_means_encoder_decoder_vocab_and_ort() {
        let root = unique_temp("host-stt");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);
        write_stt_artifacts(&binaries);

        let roots = LayoutRoots {
            binaries: binaries.clone(),
            models: models.clone(),
        };
        let status = derive_setup_status(
            linux::ARTIFACTS,
            &roots,
            chat.to_str().unwrap(),
            root.to_string_lossy().into(),
        );

        assert!(status.ready);
        assert!(status.stt_model_ready);
        assert!(status.stt_vocab_ready);
        assert!(status.binaries.ort_ready);
        assert!(status.stt_ready);
        assert!(!status.binaries.piper_ready, "piper stays out of Core gate");
        assert!(
            find_sidecar_in(&binaries, "parakeet-server").is_none(),
            "no Parakeet HTTP sidecar; Host STT ready must not require it"
        );

        // Remove vocab only — Host STT not ready; Core unchanged.
        fs::remove_file(
            binaries
                .join(constants::STT_MODEL_REL_DIR)
                .join(constants::STT_VOCAB_FILE),
        )
        .unwrap();
        let status = derive_setup_status(
            linux::ARTIFACTS,
            &roots,
            chat.to_str().unwrap(),
            root.to_string_lossy().into(),
        );
        assert!(status.ready);
        assert!(!status.stt_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn host_catalog_non_required_rows_do_not_gate_core() {
        let root = unique_temp("core-only");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);
        // No piper, vulkan libs, tts-voice, embed, or STT — Core still ready.

        let status = derive_setup_status(
            linux::ARTIFACTS,
            &LayoutRoots { binaries, models },
            chat.to_str().unwrap(),
            root.to_string_lossy().into(),
        );
        assert!(status.ready);
        assert!(!status.stt_ready);
        assert!(!status.binaries.piper_ready);
        assert!(!status.embed_model_ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn guest_catalogs_use_same_derivation_pattern() {
        let root = unique_temp("guest");
        let binaries = root.join("binaries");
        let models = root.join("models");
        fs::create_dir_all(&binaries).unwrap();
        fs::create_dir_all(&models).unwrap();
        let roots = LayoutRoots { binaries, models };

        for artifacts in [windows::ARTIFACTS, macos::ARTIFACTS] {
            let status = derive_setup_status(artifacts, &roots, "", root.to_string_lossy().into());
            assert!(!status.ready);
            assert!(!status.stt_ready);
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_setup_status_uses_current_catalog_derivation() {
        let root = unique_temp("build");
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
        assert!(status.ready);
        assert!(!status.binaries.piper_ready);
        assert!(!status.stt_ready);

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
    fn fixture_stt_not_ready_without_ort_even_with_model_files() {
        let root = unique_temp("stt-no-ort");
        let binaries = root.join("binaries");
        let models = root.join("models");
        let chat = models.join("chat.gguf");
        write_sidecar(&binaries, "llama-server");
        write_file(&chat, 128);
        let stt = binaries.join(constants::STT_MODEL_REL_DIR);
        write_file(&stt.join(constants::STT_ENCODER_FILE), 128);
        write_file(&stt.join(constants::STT_DECODER_FILE), 128);
        write_file(&stt.join(constants::STT_VOCAB_FILE), 64);

        let status = build_setup_status(
            chat.to_str().unwrap(),
            &models,
            &binaries,
            root.to_string_lossy().into(),
        );
        assert!(status.ready);
        assert!(status.stt_model_ready);
        assert!(status.stt_vocab_ready);
        assert!(!status.binaries.ort_ready);
        assert!(!status.stt_ready);

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

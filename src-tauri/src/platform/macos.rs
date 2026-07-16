//! Guest OS (macOS) Platform module — compile-safe catalog stub.
//!
//! Kept modular so opening the project on macOS does not break shared core.
//! Artifact patterns mirror binary_store; full Host STT parity is deferred.

use super::artifact::{
    ArtifactDef, ArtifactKind, ArtifactRoot, ArtifactSource, VerifyRule,
};
use super::{PlatformId, PlatformModule};
use crate::constants;

pub struct MacosPlatform;

pub static ARTIFACTS: &[ArtifactDef] = &[
    ArtifactDef {
        id: "llama-server",
        kind: ArtifactKind::Sidecar,
        root: ArtifactRoot::Binaries,
        relative_dir: "",
        filename: "llama-server",
        sidecar_name: true,
        #[cfg(target_arch = "aarch64")]
        source: ArtifactSource::GithubRelease {
            repo: "ggml-org/llama.cpp",
            pattern: "bin-macos-arm64",
        },
        #[cfg(not(target_arch = "aarch64"))]
        source: ArtifactSource::GithubRelease {
            repo: "ggml-org/llama.cpp",
            pattern: "bin-macos-x64",
        },
        verify: VerifyRule::SidecarUsable,
        required_for_core: true,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "piper",
        kind: ArtifactKind::Sidecar,
        root: ArtifactRoot::Binaries,
        relative_dir: "",
        filename: "piper",
        sidecar_name: true,
        #[cfg(target_arch = "aarch64")]
        source: ArtifactSource::GithubRelease {
            repo: "rhasspy/piper",
            pattern: "piper_macos_aarch64.tar.gz",
        },
        #[cfg(not(target_arch = "aarch64"))]
        source: ArtifactSource::GithubRelease {
            repo: "rhasspy/piper",
            pattern: "piper_macos_x86_64.tar.gz",
        },
        verify: VerifyRule::SidecarUsable,
        required_for_core: false,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "embed-model",
        kind: ArtifactKind::Model,
        root: ArtifactRoot::Models,
        relative_dir: "",
        filename: constants::EMBED_MODEL_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::EMBED_MODEL_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "stt-encoder",
        kind: ArtifactKind::Model,
        root: ArtifactRoot::Binaries,
        relative_dir: "parakeet/models",
        filename: constants::STT_ENCODER_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::STT_ENCODER_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: true,
    },
    ArtifactDef {
        id: "stt-decoder",
        kind: ArtifactKind::Model,
        root: ArtifactRoot::Binaries,
        relative_dir: "parakeet/models",
        filename: constants::STT_DECODER_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::STT_DECODER_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: true,
    },
    ArtifactDef {
        id: "stt-vocab",
        kind: ArtifactKind::Data,
        root: ArtifactRoot::Binaries,
        relative_dir: "parakeet/models",
        filename: constants::STT_VOCAB_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::STT_VOCAB_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: true,
    },
    ArtifactDef {
        id: "onnxruntime",
        kind: ArtifactKind::Data,
        root: ArtifactRoot::Binaries,
        relative_dir: "ort",
        filename: "libonnxruntime.dylib",
        sidecar_name: false,
        #[cfg(target_arch = "aarch64")]
        source: ArtifactSource::GithubRelease {
            repo: "microsoft/onnxruntime",
            pattern: "onnxruntime-osx-arm64-",
        },
        #[cfg(not(target_arch = "aarch64"))]
        source: ArtifactSource::GithubRelease {
            repo: "microsoft/onnxruntime",
            pattern: "onnxruntime-osx-x86_64-",
        },
        verify: VerifyRule::SharedLibPresent,
        required_for_core: false,
        required_for_stt: true,
    },
];

impl PlatformModule for MacosPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Macos
    }

    fn artifacts(&self) -> &'static [ArtifactDef] {
        ARTIFACTS
    }
}

//! Guest OS (Windows) Platform module — compile-safe catalog stub.
//!
//! Definitions are present so shared core can resolve ids; fetch patterns match
//! existing binary_store knowledge. Full STT parity lands when Windows becomes Host.

use super::artifact::{
    ArtifactDef, ArtifactKind, ArtifactRoot, ArtifactSource, VerifyRule,
};
use super::{PlatformId, PlatformModule};
use crate::constants;

pub struct WindowsPlatform;

pub static ARTIFACTS: &[ArtifactDef] = &[
    ArtifactDef {
        id: "llama-server",
        kind: ArtifactKind::Sidecar,
        root: ArtifactRoot::Binaries,
        relative_dir: "",
        filename: "llama-server",
        sidecar_name: true,
        source: ArtifactSource::GithubRelease {
            repo: "ggml-org/llama.cpp",
            pattern: "bin-win-cpu-x64.zip",
        },
        verify: VerifyRule::SidecarUsable,
        required_for_core: true,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "llama-vulkan-libs",
        kind: ArtifactKind::Data,
        root: ArtifactRoot::Binaries,
        relative_dir: "llama",
        filename: "ggml-vulkan.dll",
        sidecar_name: false,
        source: ArtifactSource::GithubRelease {
            repo: "ggml-org/llama.cpp",
            pattern: "bin-win-vulkan-x64.zip",
        },
        verify: VerifyRule::SharedLibPresent,
        required_for_core: false,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "piper",
        kind: ArtifactKind::Sidecar,
        root: ArtifactRoot::Binaries,
        relative_dir: "",
        filename: "piper",
        sidecar_name: true,
        source: ArtifactSource::GithubRelease {
            repo: "rhasspy/piper",
            pattern: "piper_windows_amd64.zip",
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
        filename: "onnxruntime.dll",
        sidecar_name: false,
        source: ArtifactSource::GithubRelease {
            repo: "microsoft/onnxruntime",
            pattern: "onnxruntime-win-x64-",
        },
        verify: VerifyRule::SharedLibPresent,
        required_for_core: false,
        required_for_stt: true,
    },
];

impl PlatformModule for WindowsPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Windows
    }

    fn artifacts(&self) -> &'static [ArtifactDef] {
        ARTIFACTS
    }
}

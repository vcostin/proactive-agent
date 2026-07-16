//! Host OS (Linux) Platform module — populated app-managed artifact catalog.

use super::artifact::{
    ArtifactDef, ArtifactKind, ArtifactRoot, ArtifactSource, VerifyRule,
};
use super::{PlatformId, PlatformModule};
use crate::constants;

pub struct LinuxPlatform;

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
            pattern: "bin-ubuntu-x64.tar.gz",
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
        // Filename unused for SharedLibPresent — documents the fetch asset.
        filename: "libggml-vulkan.so",
        sidecar_name: false,
        source: ArtifactSource::GithubRelease {
            repo: "ggml-org/llama.cpp",
            pattern: "bin-ubuntu-vulkan-x64.tar.gz",
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
            pattern: "piper_linux_x86_64.tar.gz",
        },
        verify: VerifyRule::SidecarUsable,
        required_for_core: false,
        required_for_stt: false,
    },
    ArtifactDef {
        id: "tts-voice",
        kind: ArtifactKind::Model,
        root: ArtifactRoot::Models,
        relative_dir: "tts",
        filename: constants::TTS_MODEL_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx",
        },
        verify: VerifyRule::Exists,
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
        id: "stt-model",
        kind: ArtifactKind::Model,
        root: ArtifactRoot::Binaries,
        relative_dir: "parakeet/models",
        filename: constants::STT_MODEL_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::STT_MODEL_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: true,
    },
    ArtifactDef {
        id: "stt-tokens",
        kind: ArtifactKind::Data,
        root: ArtifactRoot::Binaries,
        relative_dir: "parakeet/models",
        filename: constants::STT_TOKENS_FILE,
        sidecar_name: false,
        source: ArtifactSource::Url {
            url: constants::STT_TOKENS_URL,
        },
        verify: VerifyRule::Exists,
        required_for_core: false,
        required_for_stt: true,
    },
    ArtifactDef {
        id: "parakeet-server",
        kind: ArtifactKind::Sidecar,
        root: ArtifactRoot::Binaries,
        relative_dir: "",
        filename: "parakeet-server",
        sidecar_name: true,
        source: ArtifactSource::Manual,
        verify: VerifyRule::SidecarUsable,
        required_for_core: false,
        required_for_stt: true,
    },
];

impl PlatformModule for LinuxPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Linux
    }

    fn artifacts(&self) -> &'static [ArtifactDef] {
        ARTIFACTS
    }
}

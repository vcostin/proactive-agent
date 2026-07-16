//! Platform modules: per-OS single source of truth for app-managed artifact definitions.
//!
//! Host OS (Linux) has a populated catalog. Guest OS modules compile and register
//! without executing Host-only install side effects.

pub mod artifact;
pub mod linux;
pub mod macos;
pub mod windows;

pub use artifact::{
    artifact_ready, verify_catalog, ArtifactDef, ArtifactKind, ArtifactRoot, ArtifactSource,
    LayoutRoots, VerifyRule, VerifyStatus,
};

/// Identifier for the OS this build targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformId {
    Linux,
    Windows,
    Macos,
}

impl PlatformId {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }
}

/// A Platform module: catalog of app-managed artifacts for one OS.
pub trait PlatformModule: Send + Sync {
    fn id(&self) -> PlatformId;
    fn artifacts(&self) -> &'static [ArtifactDef];
}

pub fn current_module() -> &'static dyn PlatformModule {
    match PlatformId::current() {
        PlatformId::Linux => &linux::LinuxPlatform,
        PlatformId::Windows => &windows::WindowsPlatform,
        PlatformId::Macos => &macos::MacosPlatform,
    }
}

/// All registered Platform modules (Host + Guests) — for compile/registration checks.
pub fn all_modules() -> &'static [&'static dyn PlatformModule] {
    &[
        &linux::LinuxPlatform,
        &windows::WindowsPlatform,
        &macos::MacosPlatform,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_modules_register_without_host_side_effects() {
        let modules = all_modules();
        assert_eq!(modules.len(), 3);
        for m in modules {
            // Merely reading the catalog must not download or install anything.
            let _ = m.artifacts();
            assert!(!m.id().as_str().is_empty());
        }
    }

    #[test]
    fn current_module_matches_compile_target() {
        let id = current_module().id();
        #[cfg(target_os = "linux")]
        assert_eq!(id, PlatformId::Linux);
        #[cfg(target_os = "windows")]
        assert_eq!(id, PlatformId::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(id, PlatformId::Macos);
    }
}

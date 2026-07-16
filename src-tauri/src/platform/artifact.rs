//! App-managed artifact definitions and verify/"ready?" rules.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Sidecar,
    Model,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRoot {
    /// Under the app binaries directory (sidecars, STT assets).
    Binaries,
    /// Under the models directory (embed GGUF, TTS voice files).
    Models,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArtifactSource {
    /// Resolve latest GitHub release asset whose name contains `pattern`.
    GithubRelease { repo: &'static str, pattern: &'static str },
    /// Direct HTTPS URL.
    Url { url: &'static str },
    /// Not downloaded by the wizard/CLI automatically.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyRule {
    /// Path exists as a file.
    Exists,
    /// Sidecar usability gate (size + executable bit / Windows size floor).
    SidecarUsable,
    /// Directory `relative_dir` under root contains at least one shared library (.so / .dll).
    SharedLibPresent,
}

/// One app-managed artifact owned by a Platform module.
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactDef {
    pub id: &'static str,
    pub kind: ArtifactKind,
    pub root: ArtifactRoot,
    /// Directory relative to the chosen root (may be "").
    pub relative_dir: &'static str,
    /// Destination filename (or sidecar logical name resolved via `sidecar_filename`).
    pub filename: &'static str,
    /// When true, `filename` is a logical sidecar name (`llama-server`) not a literal file.
    pub sidecar_name: bool,
    pub source: ArtifactSource,
    pub verify: VerifyRule,
    /// Required for Core agent main-UI readiness.
    pub required_for_core: bool,
    /// Part of Host STT (mic → text) path.
    pub required_for_stt: bool,
}

#[derive(Debug, Clone)]
pub struct LayoutRoots {
    pub binaries: PathBuf,
    pub models: PathBuf,
}

impl LayoutRoots {
    pub fn resolve(&self, def: &ArtifactDef) -> PathBuf {
        let base = match def.root {
            ArtifactRoot::Binaries => &self.binaries,
            ArtifactRoot::Models => &self.models,
        };
        let dir = if def.relative_dir.is_empty() {
            base.clone()
        } else {
            base.join(def.relative_dir)
        };
        if def.sidecar_name {
            // Prefer nested short-name layout: binaries/llama/llama-server-…
            let short = def.filename.split('-').next().unwrap_or(def.filename);
            let triple_name = crate::sidecar_filename(def.filename);
            let nested = dir.join(short).join(&triple_name);
            if nested.exists() {
                return nested;
            }
            let alt = base.join(def.filename).join(&triple_name);
            if alt.exists() {
                return alt;
            }
            return nested;
        }
        dir.join(def.filename)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VerifyStatus {
    pub id: String,
    pub ready: bool,
    pub path: String,
}

pub fn artifact_ready(def: &ArtifactDef, roots: &LayoutRoots) -> bool {
    match def.verify {
        VerifyRule::Exists => roots.resolve(def).is_file(),
        VerifyRule::SidecarUsable => crate::sidecar_file_usable(&roots.resolve(def)),
        VerifyRule::SharedLibPresent => {
            let base = match def.root {
                ArtifactRoot::Binaries => &roots.binaries,
                ArtifactRoot::Models => &roots.models,
            };
            let dir = if def.relative_dir.is_empty() {
                base.clone()
            } else {
                base.join(def.relative_dir)
            };
            let Ok(rd) = std::fs::read_dir(&dir) else {
                return false;
            };
            rd.filter_map(Result::ok).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                e.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && (s.contains(".so") || s.ends_with(".dll"))
            })
        }
    }
}

pub fn verify_catalog(artifacts: &[ArtifactDef], roots: &LayoutRoots) -> Vec<VerifyStatus> {
    artifacts
        .iter()
        .map(|def| {
            let path = roots.resolve(def);
            VerifyStatus {
                id: def.id.to_string(),
                ready: artifact_ready(def, roots),
                path: path.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

/// Look up an artifact by id in a catalog.
pub fn find_artifact<'a>(artifacts: &'a [ArtifactDef], id: &str) -> Option<&'a ArtifactDef> {
    artifacts.iter().find(|a| a.id == id)
}

/// JSON-serializable projection for Developer setup consumers.
#[derive(Debug, Serialize)]
pub struct CatalogProjection {
    pub platform: String,
    pub artifacts: Vec<ArtifactProjection>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactProjection {
    pub id: &'static str,
    pub kind: ArtifactKind,
    pub root: ArtifactRoot,
    pub relative_dir: &'static str,
    pub filename: &'static str,
    pub sidecar_name: bool,
    pub source: ArtifactSource,
    pub required_for_core: bool,
    pub required_for_stt: bool,
}

pub fn project_catalog(platform: &str, artifacts: &[ArtifactDef]) -> CatalogProjection {
    CatalogProjection {
        platform: platform.to_string(),
        artifacts: artifacts
            .iter()
            .map(|a| ArtifactProjection {
                id: a.id,
                kind: a.kind,
                root: a.root,
                relative_dir: a.relative_dir,
                filename: a.filename,
                sidecar_name: a.sidecar_name,
                source: a.source.clone(),
                required_for_core: a.required_for_core,
                required_for_stt: a.required_for_stt,
            })
            .collect(),
    }
}

/// Ensure parent dirs exist; used by idempotent install helpers.
pub fn ensure_dest_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::linux;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pa-catalog-{label}-{nanos}"))
    }

    fn write_sidecar(root: &Path, logical: &str) {
        let filename = crate::sidecar_filename(logical);
        let short = logical.split('-').next().unwrap_or(logical);
        let dir = root.join(short);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(filename);
        fs::write(&path, vec![0u8; 64]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
    }

    #[test]
    fn linux_catalog_verify_llama_and_stt_against_fixtures() {
        let root = unique_temp("linux");
        let binaries = root.join("binaries");
        let models = root.join("models");
        write_sidecar(&binaries, "llama-server");
        fs::create_dir_all(models.join("tts")).unwrap();
        fs::write(
            binaries
                .join("parakeet")
                .join("models")
                .join(crate::constants::STT_MODEL_FILE),
            vec![0u8; 32],
        )
        .ok();
        // create parent for STT
        let stt = binaries
            .join("parakeet")
            .join("models")
            .join(crate::constants::STT_MODEL_FILE);
        fs::create_dir_all(stt.parent().unwrap()).unwrap();
        fs::write(&stt, vec![0u8; 32]).unwrap();
        fs::write(
            models.join(crate::constants::EMBED_MODEL_FILE),
            vec![0u8; 32],
        )
        .unwrap();

        let roots = LayoutRoots { binaries, models };
        let statuses = verify_catalog(linux::ARTIFACTS, &roots);
        let by_id = |id: &str| statuses.iter().find(|s| s.id == id).unwrap();

        assert!(by_id("llama-server").ready);
        assert!(!by_id("piper").ready);
        assert!(by_id("embed-model").ready);
        assert!(by_id("stt-model").ready);
        assert!(!by_id("parakeet-server").ready);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn same_definition_same_ready_answer() {
        let root = unique_temp("idem");
        let binaries = root.join("binaries");
        let models = root.join("models");
        write_sidecar(&binaries, "piper");
        let roots = LayoutRoots {
            binaries: binaries.clone(),
            models: models.clone(),
        };
        let a = verify_catalog(linux::ARTIFACTS, &roots);
        let b = verify_catalog(linux::ARTIFACTS, &roots);
        assert_eq!(a, b);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn checked_in_linux_json_matches_catalog() {
        let projection = project_catalog("linux", crate::platform::linux::ARTIFACTS);
        let expected = include_str!("../../../scripts/artifacts/linux.json");
        let expected: serde_json::Value = serde_json::from_str(expected)
            .expect("scripts/artifacts/linux.json must be valid JSON");
        let actual = serde_json::to_value(&projection).unwrap();
        assert_eq!(
            actual, expected,
            "scripts/artifacts/linux.json is out of sync — run: \
             cargo run --bin dump_artifact_catalog -- --platform linux > scripts/artifacts/linux.json"
        );
    }
}

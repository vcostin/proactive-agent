/// binary_store.rs — Download and extract sidecar binaries from GitHub releases.
///
/// Each binary (llama-server, piper, onnxruntime) has a known release pattern
/// per OS/arch. The wizard calls `download_required_binaries()` on first run so
/// the user never has to touch a terminal.

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use tauri::Emitter;

use crate::commands::DownloadProgress;
use crate::platform::ArtifactSource;

/// Resolve GitHub release (repo, pattern) for an artifact id from the Platform module.
fn catalog_github(id: &str) -> Option<(&'static str, &'static str)> {
    crate::platform::current_module()
        .artifacts()
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| match &a.source {
            ArtifactSource::GithubRelease { repo, pattern } => Some((*repo, *pattern)),
            _ => None,
        })
}

// ── Platform extract mechanics (not artifact URL/name SSOT — see `platform`) ─

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-pc-windows-msvc";
    pub const LLAMA_EXE:      &str = "llama-server.exe";
    pub const PIPER_EXE:      &str = "piper.exe";
    pub const PIPER_IS_TARGZ: bool = false;
    pub const LLAMA_IS_TARGZ: bool = false;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod platform {
    pub const TRIPLE:         &str = "aarch64-apple-darwin";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
    pub const LLAMA_IS_TARGZ: bool = true;
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-apple-darwin";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
    pub const LLAMA_IS_TARGZ: bool = true;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-unknown-linux-gnu";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
    pub const LLAMA_IS_TARGZ: bool = true;
}

// ── Public status ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct BinariesStatus {
    pub llama_ready: bool,
    pub piper_ready: bool,
    /// App-managed ONNX Runtime shared library under binaries/ort/.
    pub ort_ready: bool,
    /// Human-readable note shown in the wizard for ORT.
    pub ort_note: String,
}

pub fn check_binaries() -> BinariesStatus {
    // Delegate to the setup seam so wizard and status share one readiness answer.
    crate::setup::status::check_binaries_in(&crate::binaries_dir())
}

// ── Download entry point ──────────────────────────────────────────────────────

/// Download llama-server (required for Core agent), piper (optional TTS),
/// and the ONNX Runtime shared library (Host STT).
/// Emits `download_progress` events throughout.
/// Piper failure does not fail the overall download — TTS is out of the Host completion bar.
pub async fn download_all(app: &tauri::AppHandle) -> Result<()> {
    let client = Client::builder()
        .user_agent("proactive-agent/1.0")
        .build()?;

    if crate::find_sidecar("llama-server").is_none() {
        download_llama(&client, app).await
            .context("failed to download llama-server")?;
    } else {
        emit_done(app, "llama-server", "already present");
    }

    if crate::find_sidecar("piper").is_none() {
        if let Err(e) = download_piper(&client, app).await {
            // Optional — do not block Core agent setup on TTS fetch failure.
            emit_done(app, "piper", &format!("skipped ({e})"));
        }
    } else {
        emit_done(app, "piper", "already present");
    }

    if !crate::setup::status::ort_lib_ready_in(&crate::binaries_dir()) {
        download_ort(&client, app)
            .await
            .context("failed to download ONNX Runtime library")?;
    } else {
        emit_done(app, "onnxruntime", "already present");
    }

    Ok(())
}

// ── llama-server ──────────────────────────────────────────────────────────────

async fn download_llama(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let llama_dir = crate::binaries_dir().join("llama");
    std::fs::create_dir_all(&llama_dir)?;

    let (repo, cpu_pat) = catalog_github("llama-server").context(
        "Platform-module catalog missing llama-server GithubRelease source",
    )?;
    let gpu_pat = catalog_github("llama-vulkan-libs").map(|(_, p)| p);

    // Prefer ggml-org; ggerganov still redirects but org moved.
    let release = match github_latest(client, repo).await {
        Ok(r) => r,
        Err(_) if repo != "ggerganov/llama.cpp" => {
            github_latest(client, "ggerganov/llama.cpp").await?
        }
        Err(e) => return Err(e),
    };
    let tag = release["tag_name"].as_str().unwrap_or("unknown").to_string();

    // Step 1: CPU archive — server binary + matched shared libs (same build).
    let cpu_url = find_asset(&release, cpu_pat)
        .with_context(|| format!("no llama.cpp CPU asset matching '{cpu_pat}' in release {tag}"))?;

    let label = if platform::LLAMA_IS_TARGZ { "llama-cpu.tar.gz" } else { "llama-cpu.zip" };
    let data = fetch_with_progress(client, app, label, &cpu_url).await?;
    let dest_name = format!("llama-server-{}", platform::TRIPLE);
    #[cfg(target_os = "windows")]
    let dest_name = format!("{dest_name}.exe");

    if platform::LLAMA_IS_TARGZ {
        extract_targz_shared_libs(&data, &llama_dir, None)
            .context("extracting llama CPU shared libs")?;
        extract_targz_one(&data, platform::LLAMA_EXE, &llama_dir.join(&dest_name))
            .context("extracting llama-server binary")?;
    } else {
        extract_zip_dlls(&data, &llama_dir, None)
            .context("extracting llama CPU DLLs")?;
        extract_zip_one(&data, platform::LLAMA_EXE, &llama_dir.join(&dest_name))
            .context("extracting llama-server binary")?;
    }

    #[cfg(not(target_os = "windows"))]
    make_executable(&llama_dir.join(&dest_name))?;

    // Step 2: Vulkan backend only — never overlay libllama* from this archive
    // (mismatched sonames + old server-impl → SEGV in string_format / --version).
    if let Some(gpu_pat) = gpu_pat {
        if let Some(url) = find_asset(&release, gpu_pat) {
            let label = if platform::LLAMA_IS_TARGZ { "llama-vulkan.tar.gz" } else { "llama-vulkan.zip" };
            let data = fetch_with_progress(client, app, label, &url).await?;
            if platform::LLAMA_IS_TARGZ {
                extract_targz_shared_libs(&data, &llama_dir, Some("libggml-vulkan"))
                    .context("extracting Vulkan shared libs")?;
            } else {
                // Same rule as Linux: only the Vulkan ggml backend DLL(s).
                extract_zip_dlls(&data, &llama_dir, Some("ggml-vulkan"))
                    .context("extracting Vulkan DLLs")?;
            }
        }
    }

    emit_done(app, "llama-server", &format!("installed ({tag})"));
    Ok(())
}

// ── piper ─────────────────────────────────────────────────────────────────────

async fn download_piper(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let piper_dir = crate::binaries_dir().join("piper");
    std::fs::create_dir_all(&piper_dir)?;

    let (repo, pat) = catalog_github("piper").context(
        "Platform-module catalog missing piper GithubRelease source",
    )?;

    let release = github_latest(client, repo).await?;
    let tag = release["tag_name"].as_str().unwrap_or("unknown").to_string();

    let url = find_asset(&release, pat)
        .with_context(|| format!("no piper asset matching '{pat}' in release {tag}"))?;

    let data = fetch_with_progress(client, app, "piper.zip", &url).await?;

    let dest_name = format!("piper-{}", platform::TRIPLE);
    #[cfg(target_os = "windows")]
    let dest_name = format!("{dest_name}.exe");

    if platform::PIPER_IS_TARGZ {
        extract_targz_piper(&data, &piper_dir, &dest_name)
            .context("extracting piper from tar.gz")?;
        ensure_soname_links(&piper_dir);
    } else {
        extract_zip_piper(&data, &piper_dir, &dest_name)
            .context("extracting piper from zip")?;
    }

    #[cfg(not(target_os = "windows"))]
    make_executable(&piper_dir.join(&dest_name))?;

    emit_done(app, "piper", &format!("installed ({tag})"));
    Ok(())
}

// ── ONNX Runtime (Host STT) ───────────────────────────────────────────────────

async fn download_ort(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let ort_dir = crate::binaries_dir().join(crate::constants::ORT_LIB_REL_DIR);
    std::fs::create_dir_all(&ort_dir)?;

    let (repo, pat) = catalog_github("onnxruntime").context(
        "Platform-module catalog missing onnxruntime GithubRelease source",
    )?;

    let release = github_latest(client, repo).await?;
    let tag = release["tag_name"].as_str().unwrap_or("unknown").to_string();

    let url = find_asset(&release, pat)
        .with_context(|| format!("no onnxruntime asset matching '{pat}' in release {tag}"))?;

    let label = if url.ends_with(".zip") {
        "onnxruntime.zip"
    } else {
        "onnxruntime.tgz"
    };
    let data = fetch_with_progress(client, app, label, &url).await?;

    if url.ends_with(".zip") {
        extract_zip_dlls(&data, &ort_dir, None).context("extracting ONNX Runtime DLLs")?;
    } else {
        extract_targz_shared_libs(&data, &ort_dir, None)
            .context("extracting ONNX Runtime shared libs")?;
    }

    emit_done(app, "onnxruntime", &format!("installed ({tag})"));
    Ok(())
}

// ── GitHub API ────────────────────────────────────────────────────────────────

async fn github_latest(client: &Client, repo: &str) -> Result<serde_json::Value> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>().await?;
    Ok(resp)
}

fn find_asset(release: &serde_json::Value, pattern: &str) -> Option<String> {
    let assets = release["assets"].as_array()?;
    // Prefer non-GPU assets when the pattern matches both (e.g. onnxruntime-linux-x64-).
    let mut fallback = None;
    for a in assets {
        let name = a["name"].as_str()?;
        if !name.contains(pattern) {
            continue;
        }
        let url = a["browser_download_url"].as_str()?.to_owned();
        let lower = name.to_lowercase();
        if lower.contains("gpu") || lower.contains("cuda") {
            if fallback.is_none() {
                fallback = Some(url);
            }
            continue;
        }
        return Some(url);
    }
    fallback
}

// ── HTTP fetch with progress events ──────────────────────────────────────────

async fn fetch_with_progress(
    client: &Client,
    app: &tauri::AppHandle,
    label: &str,
    url: &str,
) -> Result<Vec<u8>> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut buf = Vec::with_capacity(total as usize);
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        let _ = app.emit("download_progress", DownloadProgress {
            filename: label.to_string(),
            downloaded,
            total,
            done: false,
            voice_id: None,
        });
    }
    let _ = app.emit("download_progress", DownloadProgress {
        filename: label.to_string(),
        downloaded,
        total,
        done: true,
        voice_id: None,
    });
    Ok(buf)
}

// ── Archive extraction ────────────────────────────────────────────────────────

/// Extract `.dll` files from a zip into `dest_dir`.
/// When `name_contains` is set, only filenames containing that substring are kept
/// (used to avoid overlaying mismatched llama DLLs from a Vulkan-only archive).
fn extract_zip_dlls(data: &[u8], dest_dir: &Path, name_contains: Option<&str>) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_lowercase();
        if !name.ends_with(".dll") {
            continue;
        }
        let filename = Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(needle) = name_contains {
            if !filename.to_lowercase().contains(&needle.to_lowercase()) {
                continue;
            }
        }
        let dest = dest_dir.join(&filename);
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// Extract a single named file from a zip.
fn extract_zip_one(data: &[u8], name_contains: &str, dest: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_lowercase();
        if entry_name.ends_with(&name_contains.to_lowercase()) {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    bail!("'{}' not found in zip", name_contains)
}

/// Extract a single named file from a tar.gz archive.
fn extract_targz_one(data: &[u8], name_contains: &str, dest: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);
    let needle = name_contains.to_lowercase();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let lower = path.to_string_lossy().to_lowercase();
        if lower.ends_with(&needle) {
            entry.unpack(dest)?;
            return Ok(());
        }
    }
    bail!("'{}' not found in tar.gz", name_contains)
}

/// Extract shared libraries (`.so` / `.so.*`) from a tar.gz into `dest_dir`,
/// then synthesise common soname symlinks (`libfoo.so.0`, `libfoo.so`).
///
/// When `name_prefix` is `Some("libggml-vulkan")`, only matching basenames are
/// extracted — used for the Vulkan backend tarball so it cannot overlay a
/// mismatched `libllama*` ABI over an older `llama-server`.
fn extract_targz_shared_libs(
    data: &[u8],
    dest_dir: &Path,
    name_prefix: Option<&str>,
) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let lower = name.to_lowercase();
        if !lower.contains(".so") {
            continue;
        }
        if let Some(prefix) = name_prefix {
            if !name.starts_with(prefix) {
                continue;
            }
        }
        let dest = dest_dir.join(&name);
        entry.unpack(&dest)?;
    }
    ensure_soname_links(dest_dir);
    Ok(())
}

/// Create `libfoo.so.N` / `libfoo.so` links for versioned `libfoo.so.N.M…` files.
fn ensure_soname_links(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() { continue; }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with("lib") || !name.contains(".so.") { continue; }
        let Some((stem, rest)) = name.split_once(".so.") else { continue };
        let major = rest.split('.').next().unwrap_or("0");
        let link_major = dir.join(format!("{stem}.so.{major}"));
        let link_plain = dir.join(format!("{stem}.so"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if !link_major.exists() {
                let _ = symlink(name, &link_major);
            }
            if !link_plain.exists() {
                let _ = symlink(name, &link_plain);
            }
        }
        let _ = (link_major, link_plain);
    }
}

/// Extract piper from a zip: binary, DLLs, and espeak-ng-data/.
fn extract_zip_piper(data: &[u8], dest_dir: &Path, dest_exe_name: &str) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name = entry.name().to_owned();
        let lower = raw_name.to_lowercase();

        // piper executable — match platform-specific name via constant
        let piper_exe_lower = platform::PIPER_EXE.to_lowercase();
        if lower.ends_with(&piper_exe_lower) {
            let dest = dest_dir.join(dest_exe_name);
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            continue;
        }
        // DLLs
        if lower.ends_with(".dll") {
            let filename = Path::new(&raw_name).file_name().unwrap_or_default();
            let dest = dest_dir.join(filename);
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            continue;
        }
        // espeak-ng-data/ — preserve the directory structure
        if lower.contains("espeak-ng-data/") && !entry.is_dir() {
            // Strip any leading path before espeak-ng-data/
            if let Some(rel) = strip_to_component(&raw_name, "espeak-ng-data") {
                let dest = dest_dir.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out = std::fs::File::create(dest)?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
    }
    Ok(())
}

/// Extract piper from a tar.gz: binary and espeak-ng-data/.
fn extract_targz_piper(data: &[u8], dest_dir: &Path, dest_exe_name: &str) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let lower = path.to_string_lossy().to_lowercase();

        if lower.ends_with(&format!("/{}", platform::PIPER_EXE)) || lower == platform::PIPER_EXE {
            let dest = dest_dir.join(dest_exe_name);
            entry.unpack(&dest)?;
            continue;
        }
        if lower.contains("espeak-ng-data/") {
            let rel = strip_to_component(&path.to_string_lossy(), "espeak-ng-data")
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let dest = dest_dir.join(&rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(&dest)?;
        }
    }
    Ok(())
}

/// Given a path like "foo/bar/espeak-ng-data/en_dict", return "espeak-ng-data/en_dict".
fn strip_to_component(path: &str, component: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let key = format!("{component}/");
    lower.find(&key).map(|pos| path[pos..].to_owned())
}

// ── Misc helpers ──────────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

fn emit_done(app: &tauri::AppHandle, label: &str, msg: &str) {
    let _ = app.emit("download_progress", DownloadProgress {
        filename: format!("{label} — {msg}"),
        downloaded: 1,
        total: 1,
        done: true,
        voice_id: None,
    });
}

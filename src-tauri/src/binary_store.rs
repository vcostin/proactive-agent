/// binary_store.rs — Download and extract sidecar binaries from GitHub releases.
///
/// Each binary (llama-server, piper) has a known release pattern per OS/arch.
/// The wizard calls `download_required_binaries()` on first run so the user
/// never has to touch a terminal.
///
/// Parakeet is intentionally excluded — it has no public release URL and
/// requires a manual PyInstaller build. That's tracked in ROADMAP § Needs Decision.

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use tauri::Emitter;

use crate::commands::DownloadProgress;

// ── Platform constants ────────────────────────────────────────────────────────

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:           &str = "x86_64-pc-windows-msvc";
    pub const LLAMA_CPU_PAT:    &str = "bin-win-cpu-x64.zip";
    pub const LLAMA_GPU_PAT:    Option<&str> = Some("bin-win-vulkan-x64.zip");
    pub const PIPER_PAT:        &str = "piper_windows_amd64.zip";
    pub const LLAMA_EXE:        &str = "llama-server.exe";
    pub const PIPER_EXE:        &str = "piper.exe";
    pub const PIPER_IS_TARGZ:   bool = false;
    pub const LLAMA_IS_TARGZ:   bool = false;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod platform {
    pub const TRIPLE:           &str = "aarch64-apple-darwin";
    // Prefer .tar.gz; find_asset matches substring so older .zip still works.
    pub const LLAMA_CPU_PAT:    &str = "bin-macos-arm64";
    pub const LLAMA_GPU_PAT:    Option<&str> = None; // Metal built-in to llama
    pub const PIPER_PAT:        &str = "piper_macos_aarch64.tar.gz";
    pub const LLAMA_EXE:        &str = "llama-server";
    pub const PIPER_EXE:        &str = "piper";
    pub const PIPER_IS_TARGZ:   bool = true;
    pub const LLAMA_IS_TARGZ:   bool = true;
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:           &str = "x86_64-apple-darwin";
    pub const LLAMA_CPU_PAT:    &str = "bin-macos-x64";
    pub const LLAMA_GPU_PAT:    Option<&str> = None;
    pub const PIPER_PAT:        &str = "piper_macos_x86_64.tar.gz";
    pub const LLAMA_EXE:        &str = "llama-server";
    pub const PIPER_EXE:        &str = "piper";
    pub const PIPER_IS_TARGZ:   bool = true;
    pub const LLAMA_IS_TARGZ:   bool = true;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:           &str = "x86_64-unknown-linux-gnu";
    // Current ggml-org releases ship Ubuntu archives as .tar.gz (not .zip).
    pub const LLAMA_CPU_PAT:    &str = "bin-ubuntu-x64.tar.gz";
    pub const LLAMA_GPU_PAT:    Option<&str> = Some("bin-ubuntu-vulkan-x64.tar.gz");
    pub const PIPER_PAT:        &str = "piper_linux_x86_64.tar.gz";
    pub const LLAMA_EXE:        &str = "llama-server";
    pub const PIPER_EXE:        &str = "piper";
    pub const PIPER_IS_TARGZ:   bool = true;
    pub const LLAMA_IS_TARGZ:   bool = true;
}

// ── Public status ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct BinariesStatus {
    pub llama_ready:    bool,
    pub piper_ready:    bool,
    /// Always false until the user manually provides it — see ROADMAP § Needs Decision
    pub parakeet_ready: bool,
    /// Human-readable note shown in the wizard for parakeet
    pub parakeet_note:  String,
}

pub fn check_binaries() -> BinariesStatus {
    BinariesStatus {
        llama_ready:    crate::find_sidecar("llama-server").is_some(),
        piper_ready:    crate::find_sidecar("piper").is_some(),
        parakeet_ready: crate::find_sidecar("parakeet-server").is_some(),
        parakeet_note: {
            #[cfg(target_os = "linux")]
            { "Linux: managed by deno task setup (auto-starts with the app).".into() }
            #[cfg(not(target_os = "linux"))]
            { "Speech-to-text requires a manual build step. See ROADMAP for options.".into() }
        },
    }
}

// ── Download entry point ──────────────────────────────────────────────────────

/// Download llama-server and piper for the current OS/arch.
/// Emits `download_progress` events throughout.
/// Parakeet is intentionally skipped.
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
        download_piper(&client, app).await
            .context("failed to download piper")?;
    } else {
        emit_done(app, "piper", "already present");
    }

    Ok(())
}

// ── llama-server ──────────────────────────────────────────────────────────────

async fn download_llama(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let llama_dir = crate::binaries_dir().join("llama");
    std::fs::create_dir_all(&llama_dir)?;

    // Prefer ggml-org; ggerganov still redirects but org moved.
    let release = match github_latest(client, "ggml-org/llama.cpp").await {
        Ok(r) => r,
        Err(_) => github_latest(client, "ggerganov/llama.cpp").await?,
    };
    let tag = release["tag_name"].as_str().unwrap_or("unknown").to_string();

    // Step 1: GPU backend libs (Windows DLLs / Linux .so from Vulkan archive)
    if let Some(gpu_pat) = platform::LLAMA_GPU_PAT {
        if let Some(url) = find_asset(&release, gpu_pat) {
            let label = if platform::LLAMA_IS_TARGZ { "llama-vulkan.tar.gz" } else { "llama-vulkan.zip" };
            let data = fetch_with_progress(client, app, label, &url).await?;
            if platform::LLAMA_IS_TARGZ {
                extract_targz_shared_libs(&data, &llama_dir)
                    .context("extracting Vulkan shared libs")?;
            } else {
                extract_zip_dlls(&data, &llama_dir)
                    .context("extracting Vulkan DLLs")?;
            }
        }
    }

    // Step 2: CPU server binary (full HTTP API on Windows; primary binary everywhere)
    let cpu_url = find_asset(&release, platform::LLAMA_CPU_PAT)
        .with_context(|| format!("no llama.cpp CPU asset matching '{}' in release {tag}", platform::LLAMA_CPU_PAT))?;

    let label = if platform::LLAMA_IS_TARGZ { "llama-cpu.tar.gz" } else { "llama-cpu.zip" };
    let data = fetch_with_progress(client, app, label, &cpu_url).await?;
    let dest_name = format!("llama-server-{}", platform::TRIPLE);
    #[cfg(target_os = "windows")]
    let dest_name = format!("{dest_name}.exe");

    if platform::LLAMA_IS_TARGZ {
        extract_targz_one(&data, platform::LLAMA_EXE, &llama_dir.join(&dest_name))
            .context("extracting llama-server binary")?;
    } else {
        extract_zip_one(&data, platform::LLAMA_EXE, &llama_dir.join(&dest_name))
            .context("extracting llama-server binary")?;
    }

    #[cfg(not(target_os = "windows"))]
    make_executable(&llama_dir.join(&dest_name))?;

    emit_done(app, "llama-server", &format!("installed ({tag})"));
    Ok(())
}

// ── piper ─────────────────────────────────────────────────────────────────────

async fn download_piper(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let piper_dir = crate::binaries_dir().join("piper");
    std::fs::create_dir_all(&piper_dir)?;

    let release = github_latest(client, "rhasspy/piper").await?;
    let tag = release["tag_name"].as_str().unwrap_or("unknown").to_string();

    let url = find_asset(&release, platform::PIPER_PAT)
        .with_context(|| format!("no piper asset matching '{}' in release {tag}", platform::PIPER_PAT))?;

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
    assets.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if name.contains(pattern) {
            a["browser_download_url"].as_str().map(str::to_owned)
        } else {
            None
        }
    })
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
        });
    }
    let _ = app.emit("download_progress", DownloadProgress {
        filename: label.to_string(),
        downloaded,
        total,
        done: true,
    });
    Ok(buf)
}

// ── Archive extraction ────────────────────────────────────────────────────────

/// Extract all `.dll` files from a zip into `dest_dir`.
fn extract_zip_dlls(data: &[u8], dest_dir: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_lowercase();
        if name.ends_with(".dll") {
            let filename = Path::new(entry.name())
                .file_name().unwrap_or_default();
            let dest = dest_dir.join(filename);
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
        }
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
fn extract_targz_shared_libs(data: &[u8], dest_dir: &Path) -> Result<()> {
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
        if lower.contains(".so") {
            let dest = dest_dir.join(&name);
            entry.unpack(&dest)?;
        }
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
    });
}

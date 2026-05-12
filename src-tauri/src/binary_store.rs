/// binary_store.rs — Download and extract sidecar binaries.
///
/// All downloads use pinned URLs (see constants.rs) — no GitHub API calls,
/// no rate limits. The wizard calls download_required_binaries() on first run.

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use tauri::Emitter;

use crate::commands::DownloadProgress;

// ── Platform constants ────────────────────────────────────────────────────────

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-pc-windows-msvc";
    pub const LLAMA_EXE:      &str = "llama-server.exe";
    pub const PIPER_EXE:      &str = "piper.exe";
    pub const PIPER_IS_TARGZ: bool = false;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod platform {
    pub const TRIPLE:         &str = "aarch64-apple-darwin";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-apple-darwin";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod platform {
    pub const TRIPLE:         &str = "x86_64-unknown-linux-gnu";
    pub const LLAMA_EXE:      &str = "llama-server";
    pub const PIPER_EXE:      &str = "piper";
    pub const PIPER_IS_TARGZ: bool = true;
}

// ── Public status ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct BinariesStatus {
    pub llama_ready: bool,
    pub piper_ready: bool,
    // parakeet_ready removed — parakeet-server.exe no longer used.
    // STT runs in-process via ort; ONNX model files downloaded in Step 2.
}

pub fn check_binaries() -> BinariesStatus {
    BinariesStatus {
        llama_ready: crate::find_sidecar("llama-server").is_some(),
        piper_ready: crate::find_sidecar("piper").is_some(),
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

    // Download the correct onnxruntime.dll for ort rc.12 — Microsoft's official
    // CPU-only build. Piper ships ORT 1.16 which hangs on load (DirectML
    // provider mismatch). The CPU-only package has no DirectML dependency.
    download_ort_dylib(&client, app).await
        .context("failed to download onnxruntime")?;

    Ok(())
}

// ── llama-server ──────────────────────────────────────────────────────────────

async fn download_llama(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    use crate::constants::*;
    let llama_dir = crate::binaries_dir().join("llama");
    std::fs::create_dir_all(&llama_dir)?;

    // Pinned URLs — no GitHub API calls, no rate limits.
    // Step 1: Vulkan DLLs (Windows only)
    #[cfg(target_os = "windows")]
    {
        let data = fetch_with_progress(client, app, "llama-server", LLAMA_VULKAN_URL_WIN).await?;
        extract_zip_dlls(&data, &llama_dir).context("extracting Vulkan DLLs")?;
    }

    // Step 2: CPU server binary
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let cpu_url = LLAMA_CPU_URL_WIN;
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let cpu_url = LLAMA_CPU_URL_MAC_ARM;
    #[cfg(target_os = "linux")]
    let cpu_url = LLAMA_CPU_URL_LINUX;

    let data = fetch_with_progress(client, app, "llama-server", cpu_url).await?;
    let dest_name = format!("llama-server-{}", platform::TRIPLE);
    #[cfg(target_os = "windows")]
    let dest_name = format!("{dest_name}.exe");

    extract_zip_one(&data, platform::LLAMA_EXE, &llama_dir.join(&dest_name))
        .context("extracting llama-server binary")?;

    #[cfg(not(target_os = "windows"))]
    make_executable(&llama_dir.join(&dest_name))?;

    emit_done(app, "llama-server", &format!("installed ({})", LLAMA_VERSION));
    Ok(())
}

// ── piper ─────────────────────────────────────────────────────────────────────

async fn download_piper(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    use crate::constants::*;
    let piper_dir = crate::binaries_dir().join("piper");
    std::fs::create_dir_all(&piper_dir)?;

    // Pinned URL — no GitHub API call needed.
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let url = PIPER_URL_WIN;
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let url = PIPER_URL_MAC_ARM;
    #[cfg(target_os = "linux")]
    let url = PIPER_URL_LINUX;

    let data = fetch_with_progress(client, app, "piper", url).await?;

    let dest_name = format!("piper-{}", platform::TRIPLE);
    #[cfg(target_os = "windows")]
    let dest_name = format!("{dest_name}.exe");

    if platform::PIPER_IS_TARGZ {
        extract_targz_piper(&data, &piper_dir, &dest_name)
            .context("extracting piper from tar.gz")?;
    } else {
        extract_zip_piper(&data, &piper_dir, &dest_name)
            .context("extracting piper from zip")?;
    }

    #[cfg(not(target_os = "windows"))]
    make_executable(&piper_dir.join(&dest_name))?;

    emit_done(app, "piper", "installed (2023.11.14-2)");
    Ok(())
}

// (GitHub API helpers removed — all downloads now use pinned URLs in constants.rs)

// ── HTTP fetch with progress events ──────────────────────────────────────────

async fn fetch_with_progress(
    client: &Client,
    app: &tauri::AppHandle,
    label: &str,
    url: &str,
) -> Result<Vec<u8>> {
    // HuggingFace and GitHub often use chunked transfer without Content-Length on GET.
    // A HEAD request up-front gives us the file size for an accurate progress bar.
    let total = client.head(url).send().await
        .ok()
        .and_then(|r| r.content_length())
        .unwrap_or(0);

    let resp = client.get(url).send().await?.error_for_status()?;
    // Fall back to GET Content-Length if HEAD didn't return one
    let total = if total > 0 { total } else { resp.content_length().unwrap_or(0) };
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

// ── onnxruntime CPU-only DLL ──────────────────────────────────────────────────

/// Download the official Microsoft ONNX Runtime CPU-only DLL for ort rc.12.
/// Stored in binaries/parakeet/ so SttClient::new() can find it via init_from().
/// Windows only — macOS/Linux use system ORT or the ort download path.
#[cfg(target_os = "windows")]
async fn download_ort_dylib(client: &Client, app: &tauri::AppHandle) -> Result<()> {
    let dest_dir = crate::binaries_dir().join("parakeet");
    std::fs::create_dir_all(&dest_dir)?;
    let dll_dest = dest_dir.join("onnxruntime.dll");
    let shared_dest = dest_dir.join("onnxruntime_providers_shared.dll");

    if dll_dest.exists() && dll_dest.metadata().map(|m| m.len()).unwrap_or(0) > 1_000_000 {
        emit_done(app, "onnxruntime", "already present");
        return Ok(());
    }

    // NuGet package is a zip — extract specific paths for the CPU-only DLLs.
    // These paths are exact matches inside Microsoft.ML.OnnxRuntime.nupkg.
    use crate::constants::{ORT_CPU_DLL_URL, ORT_CPU_DLL_PATH_IN_PKG, ORT_CPU_SHARED_PATH_IN_PKG};
    let data = fetch_with_progress(client, app, "onnxruntime", ORT_CPU_DLL_URL).await?;
    let cursor = std::io::Cursor::new(&data);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut found_dll = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_owned();
        // NuGet uses forward slashes and exact paths
        if name == ORT_CPU_DLL_PATH_IN_PKG || name.ends_with("/onnxruntime.dll") {
            let mut out = std::fs::File::create(&dll_dest)?;
            std::io::copy(&mut entry, &mut out)?;
            found_dll = true;
        } else if name == ORT_CPU_SHARED_PATH_IN_PKG || name.ends_with("/onnxruntime_providers_shared.dll") {
            let mut out = std::fs::File::create(&shared_dest)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    if !found_dll {
        anyhow::bail!("onnxruntime.dll not found inside NuGet package — unexpected package structure");
    }
    emit_done(app, "onnxruntime", "CPU-only 1.19.2 (NuGet) installed — no GPU providers");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn download_ort_dylib(_client: &Client, _app: &tauri::AppHandle) -> Result<()> {
    Ok(()) // macOS/Linux: handled differently
}

fn emit_done(app: &tauri::AppHandle, label: &str, msg: &str) {
    let _ = app.emit("download_progress", DownloadProgress {
        filename: format!("{label} — {msg}"),
        downloaded: 1,
        total: 1,
        done: true,
    });
}

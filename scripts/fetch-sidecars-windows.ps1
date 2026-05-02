#Requires -Version 5.1
<#
.SYNOPSIS
    Downloads llama-server (Vulkan), whisper-server, and required models
    for the proactive-agent on Windows x64 with an AMD/Nvidia GPU.

.NOTES
    Run once before `npm run tauri dev`.
    Re-run whenever you want to update the binaries.
#>
$ErrorActionPreference = "Stop"

$Root     = Split-Path $PSScriptRoot -Parent
$BinDir   = Join-Path $Root "binaries"
$ModelsDir = Join-Path $Root "models"

New-Item -ItemType Directory -Force -Path $BinDir   | Out-Null
New-Item -ItemType Directory -Force -Path $ModelsDir | Out-Null

$headers = @{ "User-Agent" = "proactive-agent-setup/1.0" }

function Get-LatestRelease($repo) {
    Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
}

function Download-File($url, $dest) {
    $name = Split-Path $url -Leaf
    Write-Host "  -> $name"
    Invoke-WebRequest $url -OutFile $dest -UseBasicParsing -Headers $headers
}

function Expand-And-Find($zip, $extractTo, $pattern) {
    Remove-Item $extractTo -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive $zip -DestinationPath $extractTo -Force
    $match = Get-ChildItem $extractTo -Recurse -Filter $pattern | Select-Object -First 1
    if (-not $match) { throw "Pattern '$pattern' not found in $zip" }
    $match
}

# ─── 1. llama-server  (Vulkan build — runs on AMD and Nvidia via Vulkan API) ──
Write-Host "`n[1/4] llama.cpp (Vulkan, Windows x64)"
$llamaRelease = Get-LatestRelease "ggerganov/llama.cpp"
Write-Host "      Release: $($llamaRelease.tag_name)"

$llamaAsset = $llamaRelease.assets |
    Where-Object { $_.name -match "win.*vulkan.*x64.*\.zip$" } |
    Select-Object -First 1

if (-not $llamaAsset) {
    # Fallback: look for any Windows x64 zip if Vulkan-specific not found
    $llamaAsset = $llamaRelease.assets |
        Where-Object { $_.name -match "win.*x64.*\.zip$" } |
        Select-Object -First 1
}
if (-not $llamaAsset) { throw "Cannot find a Windows llama.cpp asset. Check https://github.com/ggerganov/llama.cpp/releases" }

$tmpZip = "$env:TEMP\llama.zip"
Download-File $llamaAsset.browser_download_url $tmpZip
$llamaExe = Expand-And-Find $tmpZip "$env:TEMP\llama-extract" "llama-server.exe"

# Copy binary + all DLLs (Vulkan runtime, ggml backends, etc.)
Copy-Item $llamaExe.FullName "$BinDir\llama-server-x86_64-pc-windows-msvc.exe" -Force
Get-ChildItem $llamaExe.Directory -Filter "*.dll" |
    ForEach-Object { Copy-Item $_.FullName $BinDir -Force }
Write-Host "  OK llama-server.exe + DLLs"

# ─── 2. whisper-server ────────────────────────────────────────────────────────
Write-Host "`n[2/4] whisper.cpp (Windows x64)"
$whisperRelease = Get-LatestRelease "ggerganov/whisper.cpp"
Write-Host "      Release: $($whisperRelease.tag_name)"

$whisperAsset = $whisperRelease.assets |
    Where-Object { $_.name -match "win.*x64.*\.zip$" -and $_.name -notmatch "coreml|xcfr" } |
    Select-Object -First 1

if (-not $whisperAsset) {
    Write-Warning "Cannot find a Windows whisper.cpp asset — skipping."
    Write-Warning "Grab it manually from https://github.com/ggerganov/whisper.cpp/releases"
} else {
    $tmpWZip = "$env:TEMP\whisper.zip"
    Download-File $whisperAsset.browser_download_url $tmpWZip
    $whisperExe = Expand-And-Find $tmpWZip "$env:TEMP\whisper-extract" "whisper-server.exe"
    Copy-Item $whisperExe.FullName "$BinDir\whisper-server-x86_64-pc-windows-msvc.exe" -Force
    Get-ChildItem $whisperExe.Directory -Filter "*.dll" |
        ForEach-Object { Copy-Item $_.FullName $BinDir -Force }
    Write-Host "  OK whisper-server.exe + DLLs"
}

# ─── 3. Models ────────────────────────────────────────────────────────────────
Write-Host "`n[3/4] Models"

# Whisper base English model (~142 MB)
$whisperModel = Join-Path $ModelsDir "ggml-base.en.bin"
if (-not (Test-Path $whisperModel)) {
    Write-Host "  Downloading ggml-base.en.bin (~142 MB)..."
    Download-File `
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin" `
        $whisperModel
    Write-Host "  OK ggml-base.en.bin"
} else { Write-Host "  OK ggml-base.en.bin (already present)" }

# nomic-embed-text embedding model (~274 MB)
$embedModel = Join-Path $ModelsDir "nomic-embed-text-v1.5.Q8_0.gguf"
if (-not (Test-Path $embedModel)) {
    Write-Host "  Downloading nomic-embed-text-v1.5.Q8_0.gguf (~274 MB)..."
    Download-File `
        "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf" `
        $embedModel
    Write-Host "  OK nomic-embed-text-v1.5.Q8_0.gguf"
} else { Write-Host "  OK nomic-embed-text (already present)" }

# ─── 4. Kokoro TTS ───────────────────────────────────────────────────────────
Write-Host "`n[4/4] Kokoro TTS server"
$kokoroExe = Join-Path $BinDir "kokoro-server-x86_64-pc-windows-msvc.exe"
if (Test-Path $kokoroExe) {
    Write-Host "  OK kokoro-server.exe (already present)"
} else {
    Write-Host "  Kokoro TTS requires Python 3.10+."
    Write-Host "  Run:  scripts\build-kokoro-exe.ps1"
    Write-Host "  Or for development run the Python server directly:"
    Write-Host "    pip install kokoro-onnx soundfile fastapi uvicorn"
    Write-Host "    python scripts\kokoro_server.py --port 8083"
}

# ─── Summary ─────────────────────────────────────────────────────────────────
Write-Host "`n─────────────────────────────────────────────────────────────"
Write-Host "Binary summary:"
Get-ChildItem $BinDir -Filter "*.exe" | ForEach-Object {
    $size = "{0:N0} KB" -f ($_.Length / 1KB)
    Write-Host "  $($_.Name)  ($size)"
}
Write-Host ""
Write-Host "Model summary:"
Get-ChildItem $ModelsDir | ForEach-Object {
    $size = "{0:N1} MB" -f ($_.Length / 1MB)
    Write-Host "  $($_.Name)  ($size)"
}
Write-Host ""
Write-Host "Next: Add a chat .gguf to models/ then run:  npm run tauri dev"
Write-Host "─────────────────────────────────────────────────────────────"

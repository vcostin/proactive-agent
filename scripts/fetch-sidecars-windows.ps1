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

# ─── 1. llama-server ─────────────────────────────────────────────────────────
# Strategy: CPU build binary + Vulkan build DLLs.
# The Vulkan-only llama-server.exe has a stripped HTTP server (only /health works).
# The CPU build has the full API. It still loads Vulkan DLLs at runtime
# from its directory, so GPU inference is preserved.
Write-Host "`n[1/4] llama.cpp (CPU binary + Vulkan DLLs for GPU)"
$llamaRelease = Get-LatestRelease "ggerganov/llama.cpp"
Write-Host "      Release: $($llamaRelease.tag_name)"

$LlamaBinDir = Join-Path $BinDir "llama"
New-Item -ItemType Directory -Force -Path $LlamaBinDir | Out-Null

# Step A: Vulkan build — DLLs only (ggml-vulkan.dll etc for GPU inference)
$vulkanAsset = $llamaRelease.assets |
    Where-Object { $_.name -match "win.*vulkan.*x64.*\.zip$" } |
    Select-Object -First 1

if ($vulkanAsset) {
    Write-Host "  Downloading Vulkan DLLs..."
    Download-File $vulkanAsset.browser_download_url "$env:TEMP\llama-vulkan.zip"
    Remove-Item "$env:TEMP\llama-vulkan-extract" -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive "$env:TEMP\llama-vulkan.zip" -DestinationPath "$env:TEMP\llama-vulkan-extract" -Force
    $dllCount = 0
    Get-ChildItem "$env:TEMP\llama-vulkan-extract" -Recurse -Filter "*.dll" |
        ForEach-Object { Copy-Item $_.FullName $LlamaBinDir -Force; $dllCount++ }
    Write-Host "  OK Vulkan DLLs: $dllCount files"
} else {
    Write-Warning "Vulkan zip not found — GPU DLLs skipped (CPU inference only)"
}

# Step B: CPU build — server binary only (has full HTTP API including /v1/chat/completions)
$cpuAsset = $llamaRelease.assets |
    Where-Object { $_.name -match "win.*cpu.*x64.*\.zip$" } |
    Select-Object -First 1

if (-not $cpuAsset) { throw "Cannot find CPU llama.cpp build. Check https://github.com/ggerganov/llama.cpp/releases" }

Write-Host "  Downloading CPU server binary..."
Download-File $cpuAsset.browser_download_url "$env:TEMP\llama-cpu.zip"
$llamaExe = Expand-And-Find "$env:TEMP\llama-cpu.zip" "$env:TEMP\llama-cpu-extract" "llama-server.exe"
Copy-Item $llamaExe.FullName "$LlamaBinDir\llama-server-x86_64-pc-windows-msvc.exe" -Force
Write-Host "  OK llama-server.exe (CPU build, full API) + $dllCount Vulkan DLLs"

# ─── 2. whisper-server ────────────────────────────────────────────────────────
Write-Host "`n[2/4] whisper.cpp (Windows x64)"
$whisperRelease = Get-LatestRelease "ggerganov/whisper.cpp"
Write-Host "      Release: $($whisperRelease.tag_name)"

# whisper.cpp naming has varied across releases:
#   older: whisper-bin-win-x64.zip
#   newer: whisper-bin-x64.zip  (no "win" prefix)
#   CUDA:  whisper-cublas-*-bin-x64.zip (skip — we want CPU/default build)
$whisperAsset = $whisperRelease.assets |
    Where-Object {
        $_.name -match "x64.*\.zip$" -and
        $_.name -notmatch "coreml|xcfr|cublas|metal|ios|android|arm"
    } |
    Sort-Object { ($_.name -match "vulkan") } -Descending |   # prefer Vulkan if present
    Select-Object -First 1

if (-not $whisperAsset) {
    Write-Warning "Cannot find a Windows whisper.cpp asset — skipping."
    Write-Warning "Grab it manually from https://github.com/ggerganov/whisper.cpp/releases"
} else {
    $tmpWZip = "$env:TEMP\whisper.zip"
    Download-File $whisperAsset.browser_download_url $tmpWZip
    $whisperExe = Expand-And-Find $tmpWZip "$env:TEMP\whisper-extract" "whisper-server.exe"
    $WhisperBinDir = Join-Path $BinDir "whisper"
    New-Item -ItemType Directory -Force -Path $WhisperBinDir | Out-Null
    Copy-Item $whisperExe.FullName "$WhisperBinDir\whisper-server-x86_64-pc-windows-msvc.exe" -Force
    $wDllCount = 0
    Get-ChildItem "$env:TEMP\whisper-extract" -Recurse -Filter "*.dll" |
        ForEach-Object { Copy-Item $_.FullName $WhisperBinDir -Force; $wDllCount++ }
    Write-Host "  OK whisper/ subdirectory: whisper-server.exe + $wDllCount DLLs"
}

# ─── 3. Models ────────────────────────────────────────────────────────────────
Write-Host "`n[3/4] Models"

# Whisper small English model (~466 MB) — better accent handling than base
# Change to ggml-base.en.bin (~142 MB) if disk space is tight
$whisperModel = Join-Path $ModelsDir "ggml-small.en.bin"
if (-not (Test-Path $whisperModel)) {
    Write-Host "  Downloading ggml-small.en.bin (~466 MB, better accent support)..."
    Download-File `
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin" `
        $whisperModel
    Write-Host "  OK ggml-small.en.bin"
} else { Write-Host "  OK ggml-small.en.bin (already present)" }

# nomic-embed-text embedding model (~274 MB)
$embedModel = Join-Path $ModelsDir "nomic-embed-text-v1.5.Q8_0.gguf"
if (-not (Test-Path $embedModel)) {
    Write-Host "  Downloading nomic-embed-text-v1.5.Q8_0.gguf (~274 MB)..."
    Download-File `
        "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf" `
        $embedModel
    Write-Host "  OK nomic-embed-text-v1.5.Q8_0.gguf"
} else { Write-Host "  OK nomic-embed-text (already present)" }

# ─── 4. TTS server via sherpa-onnx (no Python required) ──────────────────────
# sherpa-onnx provides pre-built binaries with an OpenAI-compatible TTS HTTP server.
# We use it as the Kokoro replacement — our TtsClient already hits /v1/audio/speech.
Write-Host "`n[4/4] TTS server (sherpa-onnx)"

$KokoroBinDir = Join-Path $BinDir "kokoro"
New-Item -ItemType Directory -Force -Path $KokoroBinDir | Out-Null

$ttsExeDest = Join-Path $KokoroBinDir "kokoro-server-x86_64-pc-windows-msvc.exe"

if (Test-Path $ttsExeDest) {
    Write-Host "  OK kokoro-server.exe (already present)"
} else {
    # sherpa-onnx ships standalone .exe binaries (not a server).
    # We use the non-streaming TTS CLI directly as a subprocess from Rust.
    Write-Host "  Downloading sherpa-onnx TTS binary..."
    $sherpaRelease = Invoke-RestMethod "https://api.github.com/repos/k2-fsa/sherpa-onnx/releases/latest" -Headers $headers
    Write-Host "      Release: $($sherpaRelease.tag_name)"

    # Asset is named like: sherpa-onnx-non-streaming-tts-x64-v1.13.0.exe
    $sherpaAsset = $sherpaRelease.assets |
        Where-Object { $_.name -match "sherpa-onnx-non-streaming-tts-x64.*\.exe$" } |
        Select-Object -First 1

    if (-not $sherpaAsset) {
        Write-Warning "sherpa-onnx TTS binary not found."
        Write-Warning "Download manually from: https://github.com/k2-fsa/sherpa-onnx/releases"
        Write-Warning "Rename to: $ttsExeDest"
    } else {
        Download-File $sherpaAsset.browser_download_url $ttsExeDest
        Write-Host "  OK sherpa-onnx TTS binary (CLI mode, no server needed)"
    }
}

# Download a piper TTS voice model (en_US-lessac-medium, ~65 MB)
$ttsModelDir = Join-Path $ModelsDir "tts"
New-Item -ItemType Directory -Force -Path $ttsModelDir | Out-Null
$ttsModel     = Join-Path $ttsModelDir "en_US-lessac-medium.onnx"
$ttsModelJson = Join-Path $ttsModelDir "en_US-lessac-medium.onnx.json"

if (-not (Test-Path $ttsModel)) {
    Write-Host "  Downloading piper TTS voice (en_US-lessac-medium, ~65 MB)..."
    $voiceBase = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium"
    Download-File "$voiceBase/en_US-lessac-medium.onnx"      $ttsModel
    Download-File "$voiceBase/en_US-lessac-medium.onnx.json" $ttsModelJson
    Write-Host "  OK piper voice model"
} else { Write-Host "  OK piper voice model (already present)" }

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

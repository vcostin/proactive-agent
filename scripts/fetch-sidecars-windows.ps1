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

# ─── 2. Parakeet TDT STT — binary must be built separately ───────────────────
# whisper.cpp is retired. STT is now Parakeet TDT 0.6B v3 (ONNX, CPU).
# The parakeet-server binary is built once from:
#   https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai
# using: pyinstaller --onefile main.py --name parakeet-server-x86_64-pc-windows-msvc
# The SetupWizard downloads the ONNX model files at first run.
Write-Host "`n[2/4] Parakeet TDT STT"
$ParakeetBinDir = Join-Path $BinDir "parakeet"
$ParakeetModelDir = Join-Path $ParakeetBinDir "models"
New-Item -ItemType Directory -Force -Path $ParakeetBinDir | Out-Null
New-Item -ItemType Directory -Force -Path $ParakeetModelDir | Out-Null

$parakeetExe = Join-Path $ParakeetBinDir "parakeet-server-x86_64-pc-windows-msvc.exe"
if (Test-Path $parakeetExe) {
    Write-Host "  OK parakeet-server.exe (already present)"
} else {
    Write-Warning "parakeet-server.exe not found."
    Write-Warning "Build it from: https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai"
    Write-Warning "  pyinstaller --onefile main.py --name parakeet-server-x86_64-pc-windows-msvc"
    Write-Warning "  Copy dist/parakeet-server-x86_64-pc-windows-msvc.exe to $ParakeetBinDir"
}

# Parakeet model files — downloaded by SetupWizard at first run.
# Pre-download here for dev convenience.
$parakeetOnnx   = Join-Path $ParakeetModelDir "parakeet-tdt-0.6b-v3.onnx"
$parakeetTokens = Join-Path $ParakeetModelDir "parakeet-tdt-0.6b-v3-tokens.txt"
# TODO: verify URLs before release — source: groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai
if (-not (Test-Path $parakeetOnnx)) {
    Write-Host "  Parakeet model not present — will be downloaded by SetupWizard on first run"
} else { Write-Host "  OK Parakeet model files" }

# ─── 3. Models ────────────────────────────────────────────────────────────────
Write-Host "`n[3/4] Models"

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
# ─── 4. Piper TTS — cross-platform neural TTS, offline, no Python ─────────────
# Piper reads text from stdin, writes WAV. Fast, ~65MB voice, genuinely good quality.
Write-Host "`n[4/4] Piper TTS"

$PiperBinDir = Join-Path $BinDir "piper"
New-Item -ItemType Directory -Force -Path $PiperBinDir | Out-Null

$piperExe = Join-Path $PiperBinDir "piper-x86_64-pc-windows-msvc.exe"

if (Test-Path $piperExe) {
    Write-Host "  OK piper.exe (already present)"
} else {
    Write-Host "  Downloading Piper TTS binary..."
    $piperRelease = Invoke-RestMethod "https://api.github.com/repos/rhasspy/piper/releases/latest" -Headers $headers
    Write-Host "      Release: $($piperRelease.tag_name)"

    $piperAsset = $piperRelease.assets |
        Where-Object { $_.name -match "piper_windows_amd64\.zip$" } |
        Select-Object -First 1

    if (-not $piperAsset) {
        Write-Warning "Piper Windows binary not found. Check: https://github.com/rhasspy/piper/releases"
    } else {
        $tmpPiper = "$env:TEMP\piper.zip"
        Download-File $piperAsset.browser_download_url $tmpPiper
        Remove-Item "$env:TEMP\piper-extract" -Recurse -Force -ErrorAction SilentlyContinue
        Expand-Archive $tmpPiper -DestinationPath "$env:TEMP\piper-extract" -Force

        $piperBin = Get-ChildItem "$env:TEMP\piper-extract" -Recurse -Filter "piper.exe" | Select-Object -First 1
        if ($piperBin) {
            Copy-Item $piperBin.FullName $piperExe -Force
            # Copy DLLs alongside piper.exe
            Get-ChildItem $piperBin.Directory -Filter "*.dll" |
                ForEach-Object { Copy-Item $_.FullName $PiperBinDir -Force }
            # Copy espeak-ng-data/ — required for phonemization (piper fails without it)
            $espeakData = Join-Path $piperBin.Directory "espeak-ng-data"
            if (Test-Path $espeakData) {
                Copy-Item $espeakData $PiperBinDir -Recurse -Force
                Write-Host "  OK piper.exe + espeak-ng-data"
            } else {
                Write-Warning "espeak-ng-data not found in piper zip — TTS phonemization will fail"
                Write-Host "  OK piper.exe (no espeak-ng-data)"
            }
        } else {
            Write-Warning "piper.exe not found in zip"
        }
    }
}

# Piper voice model (en_US-lessac-medium — natural sounding, ~65 MB)
$ttsModelDir = Join-Path $ModelsDir "tts"
New-Item -ItemType Directory -Force -Path $ttsModelDir | Out-Null
$ttsModel     = Join-Path $ttsModelDir "en_US-lessac-medium.onnx"
$ttsModelJson = Join-Path $ttsModelDir "en_US-lessac-medium.onnx.json"

if (-not (Test-Path $ttsModel)) {
    Write-Host "  Downloading piper voice model (en_US-lessac-medium, ~65 MB)..."
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

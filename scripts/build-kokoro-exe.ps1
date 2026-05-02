#Requires -Version 5.1
<#
.SYNOPSIS
    Builds kokoro-server.exe from scripts/kokoro_server.py using PyInstaller.
    Requires Python 3.10+ in PATH.
#>
$ErrorActionPreference = "Stop"
$Root   = Split-Path $PSScriptRoot -Parent
$BinDir = Join-Path $Root "binaries"

Write-Host "Installing Python dependencies..."
pip install kokoro-onnx soundfile fastapi uvicorn pyinstaller --quiet

Write-Host "Building kokoro-server.exe with PyInstaller..."
Push-Location $Root
pyinstaller `
    --onefile `
    --name "kokoro-server" `
    --distpath $BinDir `
    --workpath "$env:TEMP\kokoro-build" `
    --specpath "$env:TEMP\kokoro-build" `
    "scripts\kokoro_server.py"
Pop-Location

$dest = Join-Path $BinDir "kokoro-server-x86_64-pc-windows-msvc.exe"
$src  = Join-Path $BinDir "kokoro-server.exe"

if (Test-Path $src) {
    Rename-Item $src $dest -Force
    Write-Host "OK  $dest"
} elseif (Test-Path $dest) {
    Write-Host "OK  $dest (already named correctly)"
} else {
    Write-Error "Build succeeded but output not found. Check PyInstaller logs."
}

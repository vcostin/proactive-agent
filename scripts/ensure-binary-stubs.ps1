# ensure-binary-stubs.ps1
# Creates 0-byte placeholder files for externalBin entries that are missing.
# This satisfies the Tauri build-time path check without fooling the runtime
# binary validator (find_sidecar() requires len > 1024 to consider a file valid).
# The wizard detects the stubs and downloads real binaries on first run.

$root = Split-Path $PSScriptRoot -Parent

$stubs = @(
    "binaries\llama\llama-server-x86_64-pc-windows-msvc.exe",
    "binaries\parakeet\parakeet-server-x86_64-pc-windows-msvc.exe",
    "binaries\piper\piper-x86_64-pc-windows-msvc.exe"
)

foreach ($rel in $stubs) {
    $full = Join-Path $root $rel
    $dir  = Split-Path $full -Parent
    if (-not (Test-Path $full)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        New-Item -ItemType File     -Force -Path $full | Out-Null
        Write-Host "  stub: $rel"
    }
}

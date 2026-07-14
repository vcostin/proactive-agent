#!/usr/bin/env bash
# OS dispatcher for sidecar/model download scripts.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

uname_s="$(uname -s)"
case "$uname_s" in
  Linux)
    exec bash "$ROOT/scripts/fetch-sidecars-linux.sh"
    ;;
  Darwin)
    exec bash "$ROOT/scripts/fetch-sidecars-macos.sh"
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    if command -v pwsh >/dev/null 2>&1; then
      exec pwsh -File "$ROOT/scripts/fetch-sidecars-windows.ps1"
    fi
    if command -v powershell >/dev/null 2>&1; then
      exec powershell -File "$ROOT/scripts/fetch-sidecars-windows.ps1"
    fi
    echo "error: Windows setup needs PowerShell (pwsh)" >&2
    exit 1
    ;;
  *)
    echo "error: unsupported OS: $uname_s" >&2
    echo "  Linux:   deno task setup:linux   (or bash scripts/fetch-sidecars-linux.sh)" >&2
    echo "  macOS:   deno task setup:mac" >&2
    echo "  Windows: deno task setup:windows (or npm run setup:windows)" >&2
    exit 1
    ;;
esac

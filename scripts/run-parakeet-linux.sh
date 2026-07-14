#!/usr/bin/env bash
# Start Parakeet TDT STT (OpenAI-compatible) on :5092 for Linux bring-up.
# The Tauri app expects http://127.0.0.1:5092/v1/audio/transcriptions
#
# Requires: uv, ffmpeg
# Usage:  deno task parakeet:linux   |   bash scripts/run-parakeet-linux.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STT_DIR="${PARAKEET_DIR:-$ROOT/.cache/parakeet-tdt}"
PORT="${PARAKEET_PORT:-5092}"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "error: ffmpeg required (pacman -S ffmpeg)" >&2
  exit 1
fi
if ! command -v uv >/dev/null 2>&1; then
  echo "error: uv required (https://github.com/astral-sh/uv)" >&2
  exit 1
fi

mkdir -p "$(dirname "$STT_DIR")"
if [[ ! -d "$STT_DIR/.git" ]]; then
  echo "Cloning Parakeet FastAPI server…"
  git clone --depth 1 \
    https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai \
    "$STT_DIR"
fi

cd "$STT_DIR"

# Prefer a known-good CPython; 3.14 is often too new for onnxruntime pins
PY="${PARAKEET_PYTHON:-3.12}"
if [[ ! -d .venv ]]; then
  echo "Creating venv with Python $PY…"
  uv venv --python "$PY" .venv
fi
# shellcheck disable=SC1091
source .venv/bin/activate

MARKER=".venv/.cpu_deps_ok"
if [[ ! -f "$MARKER" ]]; then
  echo "Installing CPU deps (torch CPU wheel — avoids multi-GB CUDA packs)…"
  # 1) CPU torch first so silero-vad does not pull nvidia packages
  uv pip install --index-url https://download.pytorch.org/whl/cpu \
    "torch" "torchaudio"
  # 2) App deps without GPU ORT
  REQ_CPU="$(mktemp)"
  grep -vE '^(onnxruntime-gpu|silero-vad)' requirements.txt > "$REQ_CPU" || true
  echo "onnxruntime==1.23.2" >> "$REQ_CPU"
  echo "silero-vad>=6.0.0" >> "$REQ_CPU"
  uv pip install -r "$REQ_CPU"
  rm -f "$REQ_CPU"
  touch "$MARKER"
  echo "Deps ready."
else
  echo "Deps already installed (.venv)."
fi

export PARAKEET_USE_GPU=false
export PARAKEET_DEFAULT_MODEL="${PARAKEET_DEFAULT_MODEL:-parakeet-tdt-0.6b-v3}"
export PARAKEET_BATCHED=0
export HF_HOME="$STT_DIR/models"
export HF_HUB_CACHE="$STT_DIR/models"
mkdir -p "$STT_DIR/models"

echo
echo "Starting Parakeet on http://127.0.0.1:${PORT}"
echo "Health: http://127.0.0.1:${PORT}/healthz"
echo "Keep this terminal open. In the app: re-toggle the mic."
echo

# Upstream Dockerfile.cpu runs server.py (Flask+Waitress), not app.py alone
if [[ -f server.py ]]; then
  exec python server.py
fi
if [[ -f app.py ]]; then
  exec python app.py
fi
echo "error: no server.py/app.py in $STT_DIR" >&2
exit 1

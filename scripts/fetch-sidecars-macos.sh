#!/usr/bin/env bash
# Downloads llama-server and whisper-server for macOS Apple Silicon (M1/M2/M3).
# Run once before `npm run tauri dev`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/binaries"
MODELS_DIR="$ROOT/models"
mkdir -p "$BIN_DIR" "$MODELS_DIR"

UA="proactive-agent-setup/1.0"

latest_release() {
    curl -fsSL -H "User-Agent: $UA" \
        "https://api.github.com/repos/$1/releases/latest"
}

download() {
    echo "  -> $(basename "$1")"
    curl -fsSL -L -H "User-Agent: $UA" -o "$2" "$1"
}

# ─── 1. llama-server (Metal backend, Apple Silicon) ───────────────────────────
echo ""
echo "[1/4] llama.cpp (macOS arm64)"
LLAMA_JSON=$(latest_release "ggerganov/llama.cpp")
LLAMA_TAG=$(echo "$LLAMA_JSON" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
echo "      Release: $LLAMA_TAG"

LLAMA_URL=$(echo "$LLAMA_JSON" | grep '"browser_download_url"' | \
    grep -i 'macos.*arm\|arm.*macos' | grep '\.zip"' | head -1 | cut -d'"' -f4)

if [ -z "$LLAMA_URL" ]; then
    echo "  WARN: no macOS ARM asset found. Trying universal build..."
    LLAMA_URL=$(echo "$LLAMA_JSON" | grep '"browser_download_url"' | \
        grep -i 'macos' | grep '\.zip"' | head -1 | cut -d'"' -f4)
fi
if [ -z "$LLAMA_URL" ]; then
    echo "  ERROR: cannot find macOS llama.cpp asset."
    echo "  Get it from https://github.com/ggerganov/llama.cpp/releases"
    exit 1
fi

TMP_LLAMA=$(mktemp -d)
download "$LLAMA_URL" "$TMP_LLAMA/llama.zip"
unzip -q "$TMP_LLAMA/llama.zip" -d "$TMP_LLAMA/out"
LLAMA_BIN=$(find "$TMP_LLAMA/out" -name "llama-server" | head -1)
[ -z "$LLAMA_BIN" ] && { echo "ERROR: llama-server not found in zip"; exit 1; }
cp "$LLAMA_BIN" "$BIN_DIR/llama-server-aarch64-apple-darwin"
chmod +x "$BIN_DIR/llama-server-aarch64-apple-darwin"
echo "  OK llama-server"

# ─── 2. whisper-server ────────────────────────────────────────────────────────
echo ""
echo "[2/4] whisper.cpp (macOS arm64)"
WHISPER_JSON=$(latest_release "ggerganov/whisper.cpp")
WHISPER_TAG=$(echo "$WHISPER_JSON" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
echo "      Release: $WHISPER_TAG"

WHISPER_URL=$(echo "$WHISPER_JSON" | grep '"browser_download_url"' | \
    grep -i 'macos' | grep '\.zip"' | grep -v 'coreml\|xcfr' | head -1 | cut -d'"' -f4)

if [ -z "$WHISPER_URL" ]; then
    echo "  WARN: cannot find macOS whisper.cpp asset — skipping."
    echo "  Get it from https://github.com/ggerganov/whisper.cpp/releases"
else
    TMP_WHISPER=$(mktemp -d)
    download "$WHISPER_URL" "$TMP_WHISPER/whisper.zip"
    unzip -q "$TMP_WHISPER/whisper.zip" -d "$TMP_WHISPER/out"
    WHISPER_BIN=$(find "$TMP_WHISPER/out" -name "whisper-server" | head -1)
    if [ -n "$WHISPER_BIN" ]; then
        cp "$WHISPER_BIN" "$BIN_DIR/whisper-server-aarch64-apple-darwin"
        chmod +x "$BIN_DIR/whisper-server-aarch64-apple-darwin"
        echo "  OK whisper-server"
    else
        echo "  WARN: whisper-server binary not found in zip"
    fi
fi

# ─── 3. Models ────────────────────────────────────────────────────────────────
echo ""
echo "[3/4] Models"

WHISPER_MODEL="$MODELS_DIR/ggml-base.en.bin"
if [ ! -f "$WHISPER_MODEL" ]; then
    echo "  Downloading ggml-base.en.bin (~142 MB)..."
    download "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin" \
        "$WHISPER_MODEL"
    echo "  OK ggml-base.en.bin"
else
    echo "  OK ggml-base.en.bin (already present)"
fi

EMBED_MODEL="$MODELS_DIR/nomic-embed-text-v1.5.Q8_0.gguf"
if [ ! -f "$EMBED_MODEL" ]; then
    echo "  Downloading nomic-embed-text-v1.5.Q8_0.gguf (~274 MB)..."
    download "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf" \
        "$EMBED_MODEL"
    echo "  OK nomic-embed-text-v1.5.Q8_0.gguf"
else
    echo "  OK nomic-embed-text (already present)"
fi

# ─── 4. Kokoro TTS ────────────────────────────────────────────────────────────
echo ""
echo "[4/4] Kokoro TTS"
if [ -f "$BIN_DIR/kokoro-server-aarch64-apple-darwin" ]; then
    echo "  OK kokoro-server (already present)"
else
    echo "  Kokoro TTS requires Python 3.10+."
    echo "  Install:  pip install kokoro-onnx soundfile fastapi uvicorn"
    echo "  Build exe: python -m PyInstaller --onefile scripts/kokoro_server.py"
    echo "  Or for dev: python scripts/kokoro_server.py --port 8083"
fi

echo ""
echo "Done. Add a chat .gguf to models/ then run: npm run tauri dev"

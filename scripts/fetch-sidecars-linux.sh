#!/usr/bin/env bash
# Downloads llama-server (CPU + Vulkan libs), Piper TTS, and base models
# for proactive-agent on Linux x86_64.
#
# Voice input (Parakeet) is skipped — no public Linux binary yet.
# Typing + TTS + chat/memory work without it.
#
# Run:  deno task setup   |   npm run setup   |   bash scripts/fetch-sidecars-linux.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/binaries"
MODELS_DIR="$ROOT/models"
TMP="${TMPDIR:-/tmp}/proactive-agent-setup-$$"
UA="proactive-agent-setup/1.0"

mkdir -p "$BIN_DIR" "$MODELS_DIR" "$TMP"
trap 'rm -rf "$TMP"' EXIT

download() {
  local url="$1" dest="$2"
  echo "  -> $(basename "$dest")"
  curl -fL --retry 3 --retry-delay 2 -A "$UA" -o "$dest" "$url"
}

github_latest_assets() {
  local repo="$1"
  curl -fsSL -A "$UA" "https://api.github.com/repos/${repo}/releases/latest"
}

find_asset_url() {
  local json="$1" pattern="$2"
  echo "$json" | python3 -c '
import json,sys
pat=sys.argv[1]
data=json.load(sys.stdin)
for a in data.get("assets", []):
    name=a.get("name") or ""
    if pat in name:
        print(a["browser_download_url"])
        break
' "$pattern"
}

release_tag() {
  echo "$1" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("tag_name","unknown"))'
}

extract_named() {
  # extract_named <archive> <name-substring> <dest-file>
  local archive="$1" needle="$2" dest="$3"
  local list member
  list="$(tar -tzf "$archive")"
  member="$(echo "$list" | grep -E "/${needle}$|^${needle}$" | head -n1 || true)"
  if [[ -z "$member" ]]; then
    echo "error: '$needle' not found in $(basename "$archive")" >&2
    return 1
  fi
  tar -xzf "$archive" -C "$TMP" "$member"
  mkdir -p "$(dirname "$dest")"
  mv "$TMP/$member" "$dest"
  chmod +x "$dest"
}

# Create libfoo.so.0 / libfoo.so soname links for versioned shared objects.
ensure_soname_links() {
  local dir="$1"
  (
    cd "$dir" || exit 0
    shopt -s nullglob
    for f in lib*.so.*; do
      [[ -f "$f" && ! -L "$f" ]] || continue
      # libfoo.so.1.2.3 → libfoo.so.1 and libfoo.so
      local so0 so
      so0="$(echo "$f" | sed -E 's/\.so\.[0-9].*/.so.0/; t; s/\.so\..*/.so.0/')"
      # Prefer real major from name when present: .so.1.52 → .so.1
      if [[ "$f" =~ \.so\.([0-9]+)(\.|$) ]]; then
        so0="${f%%.so.*}.so.${BASH_REMATCH[1]}"
      fi
      so="${f%%.so.*}.so"
      [[ -e "$so0" ]] || ln -sfn "$f" "$so0"
      [[ -e "$so" ]] || ln -sfn "$f" "$so"
    done
  )
}

# ─── 1. llama.cpp ─────────────────────────────────────────────────────────────
echo
echo "[1/3] llama.cpp (CPU server + Vulkan shared libs when available)"

LLAMA_JSON="$(github_latest_assets "ggml-org/llama.cpp" || github_latest_assets "ggerganov/llama.cpp")"
LLAMA_TAG="$(release_tag "$LLAMA_JSON")"
echo "      Release: $LLAMA_TAG"

LLAMA_BIN_DIR="$BIN_DIR/llama"
mkdir -p "$LLAMA_BIN_DIR"
DEST_LLAMA="$LLAMA_BIN_DIR/llama-server-x86_64-unknown-linux-gnu"

if [[ -x "$DEST_LLAMA" ]]; then
  echo "  OK llama-server (already present)"
else
  CPU_URL="$(find_asset_url "$LLAMA_JSON" "bin-ubuntu-x64.tar.gz")"
  if [[ -z "$CPU_URL" ]]; then
    # Prefer exact CPU archive; avoid matching ...-ubuntu-vulkan-x64...
    CPU_URL="$(find_asset_url "$LLAMA_JSON" "ubuntu-x64.tar.gz" | grep -v vulkan | head -n1 || true)"
  fi
  if [[ -z "$CPU_URL" ]]; then
    echo "error: no Ubuntu x64 llama.cpp CPU archive in $LLAMA_TAG" >&2
    exit 1
  fi

  download "$CPU_URL" "$TMP/llama-cpu.tar.gz"
  extract_named "$TMP/llama-cpu.tar.gz" "llama-server" "$DEST_LLAMA"
  echo "  OK llama-server"
fi

# Vulkan backend libs (optional — GPU inference when drivers present)
VULKAN_URL="$(find_asset_url "$LLAMA_JSON" "bin-ubuntu-vulkan-x64.tar.gz")"
if [[ -n "$VULKAN_URL" ]]; then
  echo "  Downloading Vulkan backend libs..."
  download "$VULKAN_URL" "$TMP/llama-vulkan.tar.gz"
  mkdir -p "$TMP/llama-vulkan"
  tar -xzf "$TMP/llama-vulkan.tar.gz" -C "$TMP/llama-vulkan"
  # Copy shared libraries next to llama-server (ggml-*.so, etc.)
  find "$TMP/llama-vulkan" -type f \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp -n {} "$LLAMA_BIN_DIR/" \; 2>/dev/null || true
  # Preserve any soname symlinks from the archive
  find "$TMP/llama-vulkan" \( -type l -name '*.so' -o -type l -name '*.so.*' \) \
    -exec cp -a {} "$LLAMA_BIN_DIR/" \; 2>/dev/null || true
  ensure_soname_links "$LLAMA_BIN_DIR"
  # Some builds also ship a vulkan llama-server — keep CPU one as canonical
  SO_COUNT="$(find "$LLAMA_BIN_DIR" -maxdepth 1 \( -name '*.so' -o -name '*.so.*' \) | wc -l | tr -d ' ')"
  echo "  OK Vulkan libs: $SO_COUNT shared objects in llama/"
else
  echo "  WARN: no Ubuntu Vulkan archive — CPU inference only"
  ensure_soname_links "$LLAMA_BIN_DIR"
fi

# ─── 2. Piper TTS ─────────────────────────────────────────────────────────────
echo
echo "[2/3] Piper TTS"

PIPER_BIN_DIR="$BIN_DIR/piper"
mkdir -p "$PIPER_BIN_DIR"
DEST_PIPER="$PIPER_BIN_DIR/piper-x86_64-unknown-linux-gnu"

if [[ -x "$DEST_PIPER" ]]; then
  echo "  OK piper (already present)"
else
  PIPER_JSON="$(github_latest_assets "rhasspy/piper")"
  PIPER_TAG="$(release_tag "$PIPER_JSON")"
  echo "      Release: $PIPER_TAG"
  PIPER_URL="$(find_asset_url "$PIPER_JSON" "piper_linux_x86_64.tar.gz")"
  if [[ -z "$PIPER_URL" ]]; then
    echo "error: piper_linux_x86_64.tar.gz not found" >&2
    exit 1
  fi
  download "$PIPER_URL" "$TMP/piper.tar.gz"
  mkdir -p "$TMP/piper"
  tar -xzf "$TMP/piper.tar.gz" -C "$TMP/piper"

  PIPER_SRC="$(find "$TMP/piper" -type f -name piper | head -n1 || true)"
  if [[ -z "$PIPER_SRC" ]]; then
    echo "error: piper binary not found in archive" >&2
    exit 1
  fi
  cp "$PIPER_SRC" "$DEST_PIPER"
  chmod +x "$DEST_PIPER"

  # Shared libs next to piper (onnxruntime etc.)
  PIPER_DIR="$(dirname "$PIPER_SRC")"
  find "$PIPER_DIR" -maxdepth 1 -type f \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp {} "$PIPER_BIN_DIR/" \;
  find "$PIPER_DIR" -maxdepth 1 -type l \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp -a {} "$PIPER_BIN_DIR/" \; 2>/dev/null || true
  ensure_soname_links "$PIPER_BIN_DIR"

  # espeak-ng-data is required for phonemization
  ESPEAK_SRC="$(find "$TMP/piper" -type d -name espeak-ng-data | head -n1 || true)"
  if [[ -n "$ESPEAK_SRC" ]]; then
    rm -rf "$PIPER_BIN_DIR/espeak-ng-data"
    cp -a "$ESPEAK_SRC" "$PIPER_BIN_DIR/espeak-ng-data"
    echo "  OK piper + espeak-ng-data"
  else
    echo "  WARN: espeak-ng-data missing — TTS phonemization will fail"
    echo "  OK piper (no espeak-ng-data)"
  fi
fi

# Voice model
TTS_DIR="$MODELS_DIR/tts"
mkdir -p "$TTS_DIR"
TTS_ONNX="$TTS_DIR/en_US-lessac-medium.onnx"
TTS_JSON="$TTS_DIR/en_US-lessac-medium.onnx.json"
VOICE_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium"
if [[ ! -f "$TTS_ONNX" ]]; then
  echo "  Downloading piper voice model (en_US-lessac-medium, ~65 MB)..."
  download "$VOICE_BASE/en_US-lessac-medium.onnx" "$TTS_ONNX"
  download "$VOICE_BASE/en_US-lessac-medium.onnx.json" "$TTS_JSON"
  echo "  OK piper voice model"
else
  echo "  OK piper voice model (already present)"
fi

# ─── 3. Embedding model ───────────────────────────────────────────────────────
echo
echo "[3/3] Models"

EMBED="$MODELS_DIR/nomic-embed-text-v1.5.Q8_0.gguf"
if [[ ! -f "$EMBED" ]]; then
  echo "  Downloading nomic-embed-text-v1.5.Q8_0.gguf (~274 MB)..."
  download \
    "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf" \
    "$EMBED"
  echo "  OK nomic-embed-text"
else
  echo "  OK nomic-embed-text (already present)"
fi

# ─── Parakeet note ────────────────────────────────────────────────────────────
echo
echo "[skip] Parakeet STT — no Linux sidecar yet (voice input disabled)."
echo "       Typing + memory + proactive + Piper TTS are enough for bring-up."
mkdir -p "$BIN_DIR/parakeet"

# ─── Summary ─────────────────────────────────────────────────────────────────
echo
echo "─────────────────────────────────────────────────────────────"
echo "Binaries:"
find "$BIN_DIR" -maxdepth 2 -type f -executable | sed 's/^/  /' || true
echo
echo "Models:"
find "$MODELS_DIR" -type f | while read -r f; do
  size="$(du -h "$f" | cut -f1)"
  echo "  $(basename "$f")  ($size)"
done
echo
echo "Next:"
echo "  1. Add a chat .gguf under models/"
echo "  2. deno install          # or: npm install"
echo "  3. deno task tauri dev   # or: npm run tauri -- dev"
echo "─────────────────────────────────────────────────────────────"

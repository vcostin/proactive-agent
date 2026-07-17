#!/usr/bin/env bash
# Downloads llama-server (CPU + Vulkan libs), Piper TTS, Host STT artifacts
# (Parakeet TDT ONNX + ONNX Runtime), and base models for proactive-agent on
# Linux x86_64.
#
# Artifact names / fetch sources / destinations come from the Platform-module
# projection at scripts/artifacts/linux.json (generated from Rust SSOT).
#
# Run:  deno task setup   |   npm run setup   |   bash scripts/fetch-sidecars-linux.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/binaries"
MODELS_DIR="$ROOT/models"
TMP="${TMPDIR:-/tmp}/proactive-agent-setup-$$"
UA="proactive-agent-setup/1.0"
CATALOG="$ROOT/scripts/artifacts/linux.json"

if [[ ! -f "$CATALOG" ]]; then
  echo "error: missing Platform-module catalog at $CATALOG" >&2
  echo "  regenerate: cargo run --manifest-path src-tauri/Cargo.toml --bin dump_artifact_catalog -- --platform linux > scripts/artifacts/linux.json" >&2
  exit 1
fi

mkdir -p "$BIN_DIR" "$MODELS_DIR" "$TMP"
trap 'rm -rf "$TMP"' EXIT

# ── Catalog helpers (python3 — no jq dependency) ───────────────────────────────
catalog_field() {
  # catalog_field <artifact-id> <dotted-path>
  local id="$1" expr="$2"
  python3 - "$CATALOG" "$id" "$expr" <<'PY'
import json, sys
path, art_id, expr = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.load(open(path))
art = next((a for a in data["artifacts"] if a["id"] == art_id), None)
if art is None:
    sys.exit(f"artifact id not in catalog: {art_id}")
cur = art
for part in expr.split("."):
    if not isinstance(cur, dict) or part not in cur:
        sys.exit(0)
    cur = cur[part]
if isinstance(cur, bool):
    print("true" if cur else "false")
else:
    print(cur)
PY
}

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

ensure_soname_links() {
  local dir="$1"
  (
    cd "$dir" || exit 0
    shopt -s nullglob
    for f in lib*.so.*; do
      [[ -f "$f" && ! -L "$f" ]] || continue
      local so0 so
      so0="$(echo "$f" | sed -E 's/\.so\.[0-9].*/.so.0/; t; s/\.so\..*/.so.0/')"
      if [[ "$f" =~ \.so\.([0-9]+)(\.|$) ]]; then
        so0="${f%%.so.*}.so.${BASH_REMATCH[1]}"
      fi
      so="${f%%.so.*}.so"
      [[ -e "$so0" ]] || ln -sfn "$f" "$so0"
      [[ -e "$so" ]] || ln -sfn "$f" "$so"
    done
  )
}

LLAMA_REPO="$(catalog_field llama-server source.repo)"
LLAMA_CPU_PAT="$(catalog_field llama-server source.pattern)"
LLAMA_GPU_PAT="$(catalog_field llama-vulkan-libs source.pattern)"
PIPER_REPO="$(catalog_field piper source.repo)"
PIPER_PAT="$(catalog_field piper source.pattern)"
TTS_URL="$(catalog_field tts-voice source.url)"
TTS_FILE="$(catalog_field tts-voice filename)"
EMBED_URL="$(catalog_field embed-model source.url)"
EMBED_FILE="$(catalog_field embed-model filename)"
STT_ENC_URL="$(catalog_field stt-encoder source.url)"
STT_ENC_FILE="$(catalog_field stt-encoder filename)"
STT_DEC_URL="$(catalog_field stt-decoder source.url)"
STT_DEC_FILE="$(catalog_field stt-decoder filename)"
STT_VOCAB_URL="$(catalog_field stt-vocab source.url)"
STT_VOCAB_FILE="$(catalog_field stt-vocab filename)"
ORT_REPO="$(catalog_field onnxruntime source.repo)"
ORT_PAT="$(catalog_field onnxruntime source.pattern)"
ORT_REL_DIR="$(catalog_field onnxruntime relative_dir)"

echo
echo "[1/3] llama.cpp (CPU server + Vulkan shared libs when available)"

LLAMA_JSON="$(github_latest_assets "$LLAMA_REPO" || github_latest_assets "ggerganov/llama.cpp")"
LLAMA_TAG="$(release_tag "$LLAMA_JSON")"
echo "      Release: $LLAMA_TAG"

LLAMA_BIN_DIR="$BIN_DIR/llama"
mkdir -p "$LLAMA_BIN_DIR"
DEST_LLAMA="$LLAMA_BIN_DIR/llama-server-x86_64-unknown-linux-gnu"

if [[ -x "$DEST_LLAMA" ]]; then
  echo "  OK llama-server (already present)"
  # Do not refresh Vulkan/shared libs over an existing install — a newer
  # release's libllama-common.so.* would keep soname links while
  # libllama-server-impl.so stayed old, and llama-server SEGV'd on --version.
  if [[ -f "$LLAMA_BIN_DIR/libggml-vulkan.so" || -f "$LLAMA_BIN_DIR/libggml-vulkan.so.0" ]]; then
    echo "  OK Vulkan libs (already present)"
  else
    echo "  WARN: llama-server present but libggml-vulkan.so missing — re-run after removing binaries/llama/ for a matched refresh"
  fi
else
  CPU_URL="$(find_asset_url "$LLAMA_JSON" "$LLAMA_CPU_PAT")"
  if [[ -z "$CPU_URL" ]]; then
    CPU_URL="$(find_asset_url "$LLAMA_JSON" "ubuntu-x64.tar.gz" | grep -v vulkan | head -n1 || true)"
  fi
  if [[ -z "$CPU_URL" ]]; then
    echo "error: no Ubuntu x64 llama.cpp CPU archive in $LLAMA_TAG" >&2
    exit 1
  fi

  download "$CPU_URL" "$TMP/llama-cpu.tar.gz"
  # Full CPU archive: server binary + matching libllama* / libggml* / libmtmd*
  mkdir -p "$TMP/llama-cpu"
  tar -xzf "$TMP/llama-cpu.tar.gz" -C "$TMP/llama-cpu"
  SERVER_SRC="$(find "$TMP/llama-cpu" -type f -name 'llama-server' | head -n1 || true)"
  if [[ -z "$SERVER_SRC" ]]; then
    echo "error: llama-server not found in CPU archive" >&2
    exit 1
  fi
  cp "$SERVER_SRC" "$DEST_LLAMA"
  chmod +x "$DEST_LLAMA"
  # Shared libs from the same archive (matched ABI)
  find "$TMP/llama-cpu" -type f \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp {} "$LLAMA_BIN_DIR/" \;
  find "$TMP/llama-cpu" -type l \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp -a {} "$LLAMA_BIN_DIR/" \; 2>/dev/null || true
  echo "  OK llama-server"

  if [[ -n "$LLAMA_GPU_PAT" ]]; then
    VULKAN_URL="$(find_asset_url "$LLAMA_JSON" "$LLAMA_GPU_PAT")"
    if [[ -n "$VULKAN_URL" ]]; then
      echo "  Downloading Vulkan backend libs..."
      download "$VULKAN_URL" "$TMP/llama-vulkan.tar.gz"
      mkdir -p "$TMP/llama-vulkan"
      tar -xzf "$TMP/llama-vulkan.tar.gz" -C "$TMP/llama-vulkan"
      # Only the Vulkan ggml backend — never overlay libllama* from this tarball.
      find "$TMP/llama-vulkan" -type f \( -name 'libggml-vulkan.so' -o -name 'libggml-vulkan.so.*' \) \
        -exec cp {} "$LLAMA_BIN_DIR/" \;
      echo "  OK Vulkan libs (libggml-vulkan only)"
    else
      echo "  WARN: no Ubuntu Vulkan archive — CPU inference only"
    fi
  fi
  ensure_soname_links "$LLAMA_BIN_DIR"
fi

echo
echo "[2/3] Piper TTS"

PIPER_BIN_DIR="$BIN_DIR/piper"
mkdir -p "$PIPER_BIN_DIR"
DEST_PIPER="$PIPER_BIN_DIR/piper-x86_64-unknown-linux-gnu"

if [[ -x "$DEST_PIPER" ]]; then
  echo "  OK piper (already present)"
else
  PIPER_JSON="$(github_latest_assets "$PIPER_REPO")"
  PIPER_TAG="$(release_tag "$PIPER_JSON")"
  echo "      Release: $PIPER_TAG"
  PIPER_URL="$(find_asset_url "$PIPER_JSON" "$PIPER_PAT")"
  if [[ -z "$PIPER_URL" ]]; then
    echo "error: $PIPER_PAT not found" >&2
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

  PIPER_DIR="$(dirname "$PIPER_SRC")"
  find "$PIPER_DIR" -maxdepth 1 -type f \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp {} "$PIPER_BIN_DIR/" \;
  find "$PIPER_DIR" -maxdepth 1 -type l \( -name '*.so' -o -name '*.so.*' \) \
    -exec cp -a {} "$PIPER_BIN_DIR/" \; 2>/dev/null || true
  ensure_soname_links "$PIPER_BIN_DIR"

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

TTS_DIR="$MODELS_DIR/tts"
mkdir -p "$TTS_DIR"
TTS_ONNX="$TTS_DIR/$TTS_FILE"
TTS_JSON_META="$TTS_DIR/${TTS_FILE}.json"
if [[ ! -f "$TTS_ONNX" ]]; then
  echo "  Downloading piper voice model ($TTS_FILE)..."
  download "$TTS_URL" "$TTS_ONNX"
fi
if [[ ! -f "$TTS_JSON_META" ]]; then
  echo "  Downloading piper voice config (${TTS_FILE}.json)..."
  download "${TTS_URL}.json" "$TTS_JSON_META"
fi
echo "  OK piper voice model"

echo
echo "[3/5] Models"

EMBED="$MODELS_DIR/$EMBED_FILE"
if [[ ! -f "$EMBED" ]]; then
  echo "  Downloading $EMBED_FILE..."
  download "$EMBED_URL" "$EMBED"
  echo "  OK nomic-embed-text"
else
  echo "  OK nomic-embed-text (already present)"
fi

echo
echo "[4/5] Host STT model artifacts (Parakeet TDT ONNX)"
STT_DIR="$BIN_DIR/parakeet/models"
mkdir -p "$STT_DIR"
for pair in \
  "$STT_ENC_URL|$STT_ENC_FILE" \
  "$STT_DEC_URL|$STT_DEC_FILE" \
  "$STT_VOCAB_URL|$STT_VOCAB_FILE"
do
  url="${pair%%|*}"
  file="${pair##*|}"
  dest="$STT_DIR/$file"
  if [[ -f "$dest" && $(stat -c%s "$dest" 2>/dev/null || echo 0) -gt 1024 ]]; then
    echo "  OK $file (already present)"
  else
    echo "  Downloading $file..."
    download "$url" "$dest"
    echo "  OK $file"
  fi
done

echo
echo "[5/5] ONNX Runtime (app-managed Host STT library)"
ORT_DIR="$BIN_DIR/$ORT_REL_DIR"
mkdir -p "$ORT_DIR"
if find "$ORT_DIR" -maxdepth 1 -type f \( -name '*.so' -o -name '*.so.*' \) | grep -q .; then
  echo "  OK onnxruntime (already present)"
else
  ORT_JSON="$(github_latest_assets "$ORT_REPO")"
  ORT_TAG="$(release_tag "$ORT_JSON")"
  echo "      Release: $ORT_TAG"
  ORT_URL="$(find_asset_url "$ORT_JSON" "$ORT_PAT")"
  # Prefer CPU linux-x64 tarball (exclude gpu)
  if [[ -z "$ORT_URL" ]] || [[ "$ORT_URL" == *gpu* ]]; then
    ORT_URL="$(echo "$ORT_JSON" | python3 -c '
import json,sys
data=json.load(sys.stdin)
for a in data.get("assets", []):
    name=a.get("name") or ""
    if "onnxruntime-linux-x64-" in name and "gpu" not in name and name.endswith(".tgz"):
        print(a["browser_download_url"]); break
')"
  fi
  if [[ -z "$ORT_URL" ]]; then
    echo "error: no onnxruntime-linux-x64 tarball in $ORT_TAG" >&2
    exit 1
  fi
  download "$ORT_URL" "$TMP/ort.tgz"
  mkdir -p "$TMP/ort"
  tar -xzf "$TMP/ort.tgz" -C "$TMP/ort"
  find "$TMP/ort" -type f \( -name 'libonnxruntime.so' -o -name 'libonnxruntime.so.*' \) \
    -exec cp -n {} "$ORT_DIR/" \;
  ensure_soname_links "$ORT_DIR"
  echo "  OK onnxruntime → binaries/$ORT_REL_DIR/"
fi

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
echo "  3. deno task tauri dev   # Parakeet starts automatically with the app"
echo "─────────────────────────────────────────────────────────────"

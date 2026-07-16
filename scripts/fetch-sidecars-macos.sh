#!/usr/bin/env bash
# Guest OS (macOS) Developer setup entry — modular / non-crashing stub.
#
# Full Host parity for macOS is deferred. The Platform-module catalog for macOS
# compiles in Rust; this script does not own a divergent URL list.
# Prefer regenerating a projection when implementing macOS Host setup:
#   cargo run --manifest-path src-tauri/Cargo.toml --bin dump_artifact_catalog -- --platform macos
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "macOS Developer setup: Guest OS path — not fully populated for this iteration."
echo "  Platform-module catalog: use dump_artifact_catalog --platform macos"
echo "  Project root: $ROOT"
echo "  Shared core should still compile; open Setup Wizard / Setup repair in the app"
echo "  for app-managed artifact status when running a build."
exit 0

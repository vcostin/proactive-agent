#!/usr/bin/env bash
# Prefer Deno for frontend scripts; fall back to npm (Node compatibility).
set -euo pipefail

task="${1:?usage: run-frontend.sh <dev|build|preview>}"

if command -v deno >/dev/null 2>&1; then
  exec deno task "$task"
fi

if command -v npm >/dev/null 2>&1; then
  exec npm run "$task"
fi

echo "error: need deno or npm to run frontend task '$task'" >&2
exit 1

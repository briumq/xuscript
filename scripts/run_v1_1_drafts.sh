#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPEC_DIR="$ROOT_DIR/tests/v1_1_drafts/specs"

echo "[Xu v1.1 Drafts] Checking and running specs in: $SPEC_DIR"

shopt -s nullglob
for f in "$SPEC_DIR"/*.xu; do
  if [[ "$f" == *.invalid.xu ]]; then
    echo "==> SKIP invalid (expected to fail): ${f#$ROOT_DIR/}"
    continue
  fi
  echo "==> CHECK: ${f#$ROOT_DIR/}"
  cargo run -p xu_cli --bin xu -- check "$f"
  echo "==> RUN  : ${f#$ROOT_DIR/}"
  cargo run -p xu_cli --bin xu -- run "$f"
done

echo "[Done] All draft specs executed."

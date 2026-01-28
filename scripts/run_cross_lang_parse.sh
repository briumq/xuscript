#!/usr/bin/env bash
set -euo pipefail

SCALE=${1:-5000}

echo "Python parse:" && python3 tests/benchmarks/python/parse.py --scale "$SCALE"
echo "Node.js parse:" && node tests/benchmarks/node/parse.js --scale "$SCALE"

# Prepare Xu source
TMP_DIR="tmp"
mkdir -p "$TMP_DIR"
XU_SRC="$TMP_DIR/parse_xu_${SCALE}.xu"
python3 tests/benchmarks/xu/gen_assign.py --scale "$SCALE" --out "$XU_SRC"

# Use release Xu binary to parse AST and time externally
cargo build -q -p xu_cli --bin xu --release
XU_BIN="$(pwd)/target/release/xu"

echo "Xu parse:" && python3 - <<PY
import subprocess, json, re
p = subprocess.run(["$XU_BIN","ast","--timing","$XU_SRC"], capture_output=True, text=True)
m = re.search(r"TIMING normalize=([0-9.]+)ms lex=([0-9.]+)ms parse=([0-9.]+)ms analyze=([0-9.]+)ms", p.stdout)
parse_ms = float(m.group(3)) if m else -1.0
print(json.dumps({"case":"xu-ast","scale":$SCALE,"duration_ms":parse_ms}, ensure_ascii=False))
PY

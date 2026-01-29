#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo clean

rm -rf target
rm -rf tmp

rm -f err.txt node_bench.txt py_bench.txt
rm -f injected_*.js injected_*.py
rm -f tmp_*.xu tmp_*.py tmp_*.js tmp_*.txt

#!/usr/bin/env bash
set -euo pipefail

cargo test -q -p xu_runtime --tests perf_lexer_parser -- --ignored --nocapture
cargo test -q -p xu_runtime --tests perf_runtime_exec -- --ignored --nocapture
echo "Perf tests finished"

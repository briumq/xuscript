# Cross-language Benchmarks

- Scenarios: loop, dict, dict-intkey, string, string-builder, dict-hot, struct-method
- Scale: pass `--scale <N>` where N is 5000 or 10000 (recommended)

## Python
- Entry: `benchmarks/python/bench.py`
- Run: `python3 benchmarks/python/bench.py --scale 5000`
- Output: JSON lines per scenario with `duration_ms`

## Node.js
- Entry: `benchmarks/node/bench.js`
- Run: `node benchmarks/node/bench.js --scale 5000`
- Output: JSON lines per scenario with `duration_ms`

## Cross-language report (Python vs Node.js vs Xu)
- Run: `python3 tests/benchmarks/py_node_report.py --runs 10 --scales 5000,10000`
- Output: `tests/benchmarks/report.md`
- Columns:
  - median/p95/min/max/stdev（ms）
  - op/s（按 scale / median 推导）
  - jitter（(max-min)/median，分别列出 P/N/X）
  - winner（median 最小）

## Xu
- Use release binary for stable numbers:
- Script: `bash scripts/run_cross_lang_bench.sh 5000|10000`
- Cases added:
  - func-call、branch-heavy、list-push-pop、dict-miss、dict-update-hot、string-unicode、string-scan

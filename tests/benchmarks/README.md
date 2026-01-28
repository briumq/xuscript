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

## Xu
- Use release binary for stable numbers:
- Script: `bash scripts/run_cross_lang_bench.sh 5000|10000`

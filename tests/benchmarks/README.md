# Cross-language Benchmarks

Performance comparison between Python, Node.js, and Xu.

## Directory Structure

```
tests/benchmarks/
├── README.md              # This file
├── py_node_report.py      # Main benchmark report generator
├── compare_reports.py     # Compare historical reports
├── report.md              # Latest benchmark report
├── history/               # Historical benchmark data (JSON, keeps latest 10)
├── python/
│   ├── bench.py           # Python benchmark suite
│   └── parse.py           # Python parse benchmark
├── node/
│   ├── bench.js           # Node.js benchmark suite
│   └── parse.js           # Node.js parse benchmark
└── xu/
    ├── bench.xu           # Xu benchmark suite
    ├── gen_assign.py      # Generate Xu parse test code
    ├── gc_leak_test.xu    # GC leak test (dict)
    └── gc_pressure.xu     # GC pressure test (list)
```

## Benchmark Scenarios

| Category | Cases |
|----------|-------|
| Loop | `loop` |
| Dict | `dict`, `dict-intkey`, `dict-hot`, `dict-miss`, `dict-update-hot` |
| String | `string`, `string-builder`, `string-unicode`, `string-scan` |
| Function | `struct-method`, `func-call`, `branch-heavy` |
| Collection | `list-push-pop` |

## Running Benchmarks

### Quick Run (recommended)

```bash
# Run with 1M scale (full benchmark)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 1000000

# Run with 500K scale (faster)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 500000

# Output: tests/benchmarks/report.md
```

### Individual Language Benchmarks

```bash
# Python
python3 tests/benchmarks/python/bench.py --scale 100000

# Node.js
node tests/benchmarks/node/bench.js --scale 100000

# Xu
BENCH_SCALE=100000 ./target/release/xu run tests/benchmarks/xu/bench.xu
```

### Cross-language Script

```bash
# Run all three languages with specified scale
bash scripts/run_cross_lang_bench.sh 500000
```

## Report Columns

| Column | Description |
|--------|-------------|
| median (ms) | Median execution time |
| p95 (ms) | 95th percentile execution time |
| op/s | Operations per second (scale / median) |
| jitter | Stability: (max-min)/median for P(ython)/N(ode)/X(u) |
| mem (MB) | Peak memory usage (RSS) |
| winner | Language with lowest median time |

## Compare Historical Reports

```bash
# Compare latest two benchmark runs
python3 tests/benchmarks/compare_reports.py

# Compare specific files
python3 tests/benchmarks/compare_reports.py --old history/bench_xxx.json --new history/bench_yyy.json
```

## GC Tests

```bash
# Test GC with dict allocation/deallocation
./target/release/xu run tests/benchmarks/xu/gc_leak_test.xu

# Test GC under list pressure
./target/release/xu run tests/benchmarks/xu/gc_pressure.xu
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BENCH_SCALE` | 500000 | Number of iterations |
| `BENCH_SMOKE` | 0 | Set to 1 for quick smoke test |
| `BENCH_MAX_MEMORY_MB` | 2048 | Memory limit (MB) |
| `BENCH_SINGLE_TIMEOUT` | 600 | Single run timeout (seconds) |
| `BENCH_TOTAL_TIMEOUT` | 1800 | Total timeout (seconds) |

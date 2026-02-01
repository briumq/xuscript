# Cross-language Benchmarks

Performance comparison between Python, Node.js, and Xu.

## Directory Structure

```
tests/benchmarks/
├── README.md              # This file
├── py_node_report.py      # Main benchmark report generator
├── compare_reports.py     # Compare historical reports
├── report.md              # Latest benchmark report
├── history/               # Historical benchmark data (JSON)
├── python/
│   ├── bench.py           # Python benchmark suite
│   └── parse.py           # Python parse benchmark
├── node/
│   ├── bench.js           # Node.js benchmark suite
│   └── parse.js           # Node.js parse benchmark
└── xu/
    ├── full_suite.xu      # Xu benchmark suite (main)
    ├── gen_assign.py      # Generate Xu parse test code
    ├── gc_leak_test.xu    # GC leak test (dict)
    └── gc_pressure.xu     # GC pressure test (list)
```

## Benchmark Scenarios

- loop, dict, dict-intkey, dict-hot, dict-miss, dict-update-hot
- string, string-builder, string-unicode, string-scan
- struct-method, func-call, branch-heavy, list-push-pop

## Running Benchmarks

### Quick Run (recommended)

```bash
# Run all benchmarks with default scale (500000)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 500000

# Output: tests/benchmarks/report.md
```

### Individual Language Benchmarks

```bash
# Python
python3 tests/benchmarks/python/bench.py --scale 5000

# Node.js
node tests/benchmarks/node/bench.js --scale 5000

# Xu (via environment variable)
BENCH_SCALE=5000 ./target/release/xu run tests/benchmarks/xu/full_suite.xu
```

### Cross-language Script

```bash
bash scripts/run_cross_lang_bench.sh 500000
```

## Report Columns

- **median/p95**: Execution time in milliseconds
- **op/s**: Operations per second (scale / median)
- **jitter**: (max-min)/median for P(ython)/N(ode)/X(u)
- **winner**: Language with lowest median time

## GC Tests

```bash
# Test GC with dict allocation/deallocation
./target/release/xu run tests/benchmarks/xu/gc_leak_test.xu

# Test GC under list pressure
./target/release/xu run tests/benchmarks/xu/gc_pressure.xu
```

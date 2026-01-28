# Xu Language Test Guide (English-only)

This directory contains the test suites for Xu Language.

## Structure

- specs/: Language feature tests (assertions, correctness)
- edge/: Compiler/VM edge cases (AST vs VM consistency)
- integration/: Integration examples (compare against Golden Files in crates/xu_runtime/tests/golden/integration)
- benchmarks/: Performance benchmark suites

## Run Tests

Unified runner: crates/xu_runtime/tests/runner.rs

Run all correctness tests:

```bash
cargo test -p xu_runtime --test runner
```

Update Golden Files:

```bash
XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner
```

## Benchmarks

Smoke test (small scale):

```bash
cargo test -p xu_runtime --test run_benchmarks
```

Full benchmarks:

```bash
cargo test -p xu_runtime --test perf_benchmarks -- --ignored
```

## Scripts

- scripts/run_cross_lang_bench.sh [SCALE]
- scripts/bench_report.py

## Add New Tests

1. specs/: add .xu files with assertions
2. edge/: add VM/AST consistency cases
3. integration/: add examples and update Golden via XU_UPDATE_GOLDEN=1

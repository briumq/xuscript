# Xu Lang Testing Guide

[中文文档](README_zh.md)

This directory contains the test suite for Xu Language.

## Directory Structure

*   **`specs/`**: Language specification tests. These verify that language features work as expected.
    *   Tests run with `RunOnly` strategy (check for successful execution and assertions).
*   **`edge/`**: Compiler and VM edge case tests.
    *   Tests run with `AstVsVm` strategy (compare AST interpreter output with VM bytecode execution output).
*   **`xu/`**: Integration tests and migrated examples.
    *   Tests run with `RunAndCompare` strategy (compare output against Golden Files).
    *   Golden files are located in `crates/xu_runtime/tests/golden/xu/`.
*   **`benchmarks/`**: Performance benchmarks.
    *   `xu/suite.xu`: Unified benchmark suite file.

## Running Tests

We use a unified test runner located in `crates/xu_runtime/tests/runner.rs`.

### Basic Usage

Run all correctness tests:

```bash
cargo test -p xu_runtime --test runner
```

### Updating Golden Files

If you add new tests or expect output changes, update the golden files:

```bash
XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner
```

### Benchmarks

**Smoke Test (Fast CI check):**
Runs the benchmark suite with small size (N=500).

```bash
cargo test -p xu_runtime --test run_benchmarks
```

**Full Performance Benchmark:**
Runs the benchmark suite with full sizes (N=5000, 10000).

```bash
cargo test -p xu_runtime --test perf_benchmarks -- --ignored
```

## Scripts

Scripts in `scripts/` directory facilitate cross-language benchmarking and reporting.

*   **`scripts/run_cross_lang_bench.sh [SCALE]`**: Runs benchmarks for Python, Node.js, and Xu (via `xu/suite.xu`) at a given scale.
*   **`scripts/bench_report.py`**: Orchestrates multiple runs of `run_cross_lang_bench.sh` and generates a Markdown report in `docs/`.

Example usage:

```bash
# Generate report
python3 scripts/bench_report.py
```

## Adding New Tests

1.  **Language Features**: Add `.xu` file to `tests/specs/`. Use `断言(...)` for checks.
2.  **Compiler/VM Edge Cases**: Add `.xu` file to `tests/edge/`.
3.  **Integration/Examples**: Add `.xu` file to `tests/xu/`.
    *   Run `XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner` to generate initial golden file.

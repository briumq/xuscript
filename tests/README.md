# Xu Lang Testing Guide

[中文文档](README_zh.md)

This directory contains the test suite for Xu Language.

## Directory Structure

*   **`specs/`**: Language specification tests. These verify that language features work as expected.
    *   Tests run with `RunOnly` strategy (check for successful execution and assertions).
*   **`edge/`**: Compiler and VM edge case tests.
    *   Tests run with `AstVsVm` strategy (compare AST interpreter output with VM bytecode execution output).
*   **`integration/`**: Integration tests (golden-based).
    *   Tests run with `RunAndCompare` strategy (compare output against Golden Files).
    *   Golden files are located in `crates/xu_runtime/tests/golden/integration/`.
*   **`integration_en/`**: Optional English-only integration tests (golden-based).
    *   Golden files are located in `crates/xu_runtime/tests/golden/integration_en/`.
*   **`benchmarks/`**: Performance benchmarks.
    *   `xu/suite.xu`: Unified benchmark suite file.
*   **`../examples/`**: Examples (also part of the golden-based baseline).
    *   Golden files are located in `crates/xu_runtime/tests/golden/examples/`.

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

### Controlling Suites

The unified runner supports environment variables:

*   `XU_TEST_EXAMPLES=0|false`: Skip the whole `examples/` suite (useful for first-time bootstrap).
*   `XU_TEST_EXAMPLES_INCLUDE_EXPECT_FAIL=1|true`: Also run examples listed in `examples/manifest.json` `run_expect_fail`.
*   `XU_TEST_EDGE=0|false`: Skip `tests/edge/` suite.
*   `XU_TEST_DRAFTS=1`: Enable `tests/specs/v1_1_drafts/` suite (if present).
*   `XU_TEST_ONLY=<substr>`: Run only cases whose file stem or suite name contains `<substr>`.
*   `XU_TEST_SKIP=<csv>`: Skip cases whose path contains any item in `<csv>` (e.g. `csv_importer,large/`).

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
3.  **Integration Tests**: Add `.xu` file to `tests/integration/`.
4.  **Examples**: Add `.xu` file under `examples/` (avoid `examples/**/modules/` for entry programs).
    *   Run `XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner` to generate initial golden file.

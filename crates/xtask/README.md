# xtask

Development task runner for the XuScript project.

## Overview

This crate provides a Rust-based task runner for common development operations, avoiding shell script complexity and platform differences.

## Usage

```bash
cargo run -p xtask -- <command>
```

## Commands

| Command | Description |
|---------|-------------|
| `verify` | Run full CI checks (fmt + lint + test + examples) |
| `fmt` | Check code formatting |
| `clippy` | Run clippy linter |
| `lint` | Strict lint (clippy::all + perf + nursery) |
| `test` | Run workspace tests |
| `check-unused` | Check unused dependencies (requires cargo-udeps) |
| `examples` | Verify all example programs |
| `codegen-examples` | Test JS/Python code generation |
| `slim-baseline` | Generate slim binary baseline |
| `perf [update-baseline]` | Run performance tests |
| `bench-report [scales]` | Generate benchmark report |

## Examples

```bash
# Full verification (recommended before commit)
cargo run -p xtask -- verify

# Run tests only
cargo run -p xtask -- test

# Check formatting
cargo run -p xtask -- fmt

# Run strict linter
cargo run -p xtask -- lint

# Verify examples compile and run
cargo run -p xtask -- examples

# Run performance tests
cargo run -p xtask -- perf

# Update performance baseline
cargo run -p xtask -- perf update-baseline

# Generate benchmark report with 1M scale
cargo run -p xtask -- bench-report 1000000
```

## Verify Command

The `verify` command runs the complete CI pipeline:

1. `cargo fmt --check` - Code formatting
2. `cargo clippy -D warnings` - Linting with strict warnings
3. `cargo test --workspace` - All workspace tests
4. Example verification - Check and run all examples
5. Optional projects (if `XU_PERF=1` or `XU_BENCH=1`)

This is the recommended command to run before committing changes.

## Source Files

| File | Description |
|------|-------------|
| `src/main.rs` | Entry point and command dispatch |
| `src/process.rs` | Process execution utilities |
| `src/bench.rs` | Benchmark report generation |
| `src/perf.rs` | Performance testing |
| `src/slim.rs` | Slim binary baseline |

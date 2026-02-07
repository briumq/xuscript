# xtask

Development task runner for the XuScript project.

## Overview

This crate provides a Rust-based task runner for common development operations, avoiding shell script complexity and platform differences.

## Usage

```bash
cargo run -p xtask -- <command>

# Or with alias
cargo xtask <command>
```

## Commands

| Command | Description |
|---------|-------------|
| `verify` | Run full CI checks (fmt + clippy + test) |
| `fmt` | Check code formatting |
| `clippy` / `lint` | Run linter |
| `test` | Run all tests |
| `examples` | Verify example programs |
| `perf` | Run performance benchmarks |

## Examples

```bash
# Full verification (recommended before commit)
cargo run -p xtask -- verify

# Just run tests
cargo run -p xtask -- test

# Check formatting
cargo run -p xtask -- fmt

# Run linter
cargo run -p xtask -- clippy
```

## Verify Command

The `verify` command runs the complete CI pipeline:

1. `cargo fmt --check` - Code formatting
2. `cargo clippy` - Linting
3. `cargo test` - All tests
4. Example verification

This is the recommended command to run before committing changes.

## Source

Entry point: `src/main.rs`

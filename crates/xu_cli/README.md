# xu_cli

Command-line interface for XuScript.

## Overview

This crate provides the `xu` command-line tool for compiling and running XuScript programs.

## Installation

```bash
cargo build --release
# Binary at: target/release/xu
```

## Commands

| Command | Description |
|---------|-------------|
| `xu run <file>` | Execute a Xu script |
| `xu check <file>` | Syntax and type check |
| `xu ast <file>` | Print AST |
| `xu tokens <file>` | Print token stream |

## Usage Examples

```bash
# Run a script
xu run examples/01_basics.xu

# Run with arguments
xu run script.xu -- arg1 arg2

# Check for errors
xu check script.xu

# Print AST with timing info
xu ast --timing script.xu

# Print tokens (excluding newlines)
xu tokens script.xu
```

## Options

### Global Options

| Option | Description |
|--------|-------------|
| `--no-diags` | Suppress diagnostic output |

### Run Options

| Option | Description |
|--------|-------------|
| `--` | Separator for script arguments |

### AST Options

| Option | Description |
|--------|-------------|
| `--timing` | Show parse timing information |

## Architecture

```
xu_cli
   ├── xu_driver (compilation)
   ├── xu_runtime (execution)
   └── xu_syntax (diagnostics)
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation or runtime error |

## Tests

Located in `crates/xu_cli/tests/`:
- CLI behavior tests
- Circular import detection
- Strict mode tests

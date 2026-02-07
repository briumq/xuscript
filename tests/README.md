# XuScript Testing Guide

This directory contains the test suite for XuScript.

## Directory Structure

```
tests/
├── README.md              # This file
├── specs/                 # Language specification tests
├── edge/                  # Compiler and VM edge case tests
├── integration/           # Integration tests (golden-based)
├── benchmarks/            # Performance benchmarks (Python/Node.js/Xu)
├── fixtures/              # Test fixtures and data files
└── module_static_test/    # Module static analysis tests
```

### Test Directories

| Directory | Strategy | Description |
|-----------|----------|-------------|
| `specs/` | RunOnly | Language feature tests, verify successful execution |
| `edge/` | AstVsVm | Compare AST interpreter vs VM bytecode output |
| `integration/` | RunAndCompare | Compare output against golden files |
| `benchmarks/` | - | Cross-language performance benchmarks |

## Running Tests

We use a unified test runner located in `crates/xu_runtime/tests/runner.rs`.

### Basic Usage

```bash
# Run all correctness tests
cargo test -p xu_runtime --test runner

# Run with verbose output
cargo test -p xu_runtime --test runner -- --nocapture
```

### Updating Golden Files

If you add new tests or expect output changes:

```bash
XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `XU_UPDATE_GOLDEN=1` | Update golden files |
| `XU_TEST_EXAMPLES=0` | Skip examples suite |
| `XU_TEST_EDGE=0` | Skip edge tests |
| `XU_TEST_ONLY=<substr>` | Run only matching tests |
| `XU_TEST_SKIP=<csv>` | Skip tests matching patterns |

## Performance Benchmarks

Benchmarks compare Python, Node.js, and Xu performance.

### Quick Run

```bash
# Run with 1M scale (recommended)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 1000000

# Run with 500K scale (faster)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 500000
```

### Output

- Report: `tests/benchmarks/report.md`
- History: `tests/benchmarks/history/` (keeps latest 10 records)

### Benchmark Cases

| Category | Cases |
|----------|-------|
| Loop | `loop` |
| Dict | `dict`, `dict-intkey`, `dict-hot`, `dict-miss`, `dict-update-hot` |
| String | `string`, `string-builder`, `string-unicode`, `string-scan` |
| Function | `struct-method`, `func-call`, `branch-heavy` |
| Collection | `list-push-pop` |

See `tests/benchmarks/README.md` for detailed benchmark documentation.

## Adding New Tests

1. **Language Features**: Add `.xu` file to `tests/specs/`
   - Use `断言(...)` or `assert(...)` for checks
   - Suffix `.invalid.xu` for expected parse/compile errors
   - Suffix `.panic.xu` for expected runtime panics

2. **Edge Cases**: Add `.xu` file to `tests/edge/`
   - Tests compare AST vs VM execution

3. **Integration Tests**: Add `.xu` file to `tests/integration/`
   - Run `XU_UPDATE_GOLDEN=1 cargo test` to generate golden file

## Test File Naming Conventions

| Suffix | Meaning |
|--------|---------|
| `.xu` | Normal test, expects success |
| `.invalid.xu` | Expects parse/compile error |
| `.panic.xu` | Expects runtime panic |

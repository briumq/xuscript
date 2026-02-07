# XuScript v0.1.2

XuScript (Xu 脚本) is a strongly-typed, structured scripting language designed to be "structured, unambiguous, and executable". It provides a complete Rust implementation with CLI tools.

## Features

- **Strong typing** with type inference
- **First-class functions** and closures
- **Pattern matching** with exhaustiveness checking
- **Structs and enums** with methods
- **Module system** with imports/exports
- **Dual execution**: AST interpreter and bytecode VM

## Quick Start

### Prerequisites

- Rust/Cargo (stable)

### Installation

```bash
git https://github.com/briumq/xuscript.git
cd xuscript
cargo build --release
```

### Usage

```bash
# Run a script
./target/release/xu run examples/01_basics.xu

# Check syntax
./target/release/xu check examples/02_control_flow.xu

# Print AST
./target/release/xu ast examples/01_basics.xu

# Print tokens
./target/release/xu tokens examples/01_basics.xu
```

## Language Overview

```xu
use "math"

func main() {
    let list = [1, 2, 3]
    if list.len() > 0 {
        println("List is not empty")
    }

    for i in list {
        println("Item: {i}")
    }
}
```

### Types

| Type | Example |
|------|---------|
| `int` | `42`, `-1` |
| `float` | `3.14`, `-0.5` |
| `bool` | `true`, `false` |
| `string` | `"hello"`, `"value: {x}"` |
| `list` | `[1, 2, 3]` |
| `dict` | `{"a": 1, "b": 2}` |
| `Option` | `Option#some(x)`, `Option#none` |

### Control Flow

```xu
// If-else
if x > 0 {
    println("positive")
} else if x < 0 {
    println("negative")
} else {
    println("zero")
}

// Match
match value {
    Option#some(x) { println("Got: {x}") }
    Option#none { println("Nothing") }
}

// Loops
for i in 0..10 { println(i) }
for item in list { println(item) }
while condition { ... }
```

### Functions

```xu
func add(a: int, b: int) -> int {
    return a + b
}

// Anonymous functions
let double = func(x: int) -> int { return x * 2 }
```

### Structs and Methods

```xu
struct Point {
    x: int,
    y: int,
}

impl Point {
    func new(x: int, y: int) -> Point {
        return Point { x: x, y: y }
    }

    func distance(self) -> float {
        return math.sqrt(self.x * self.x + self.y * self.y)
    }
}
```

## Project Structure

```
xuscript/
├── crates/              # Rust crates
├── examples/            # Example programs
├── tests/               # Test suites
├── stdlib/              # Standard library
└── docs/                # Documentation (Chinese)
```

### Crates

| Crate | Description |
|-------|-------------|
| [xu_syntax](crates/xu_syntax/) | Core types: Source, Span, Token, Diagnostic |
| [xu_lexer](crates/xu_lexer/) | Lexical analysis and source normalization |
| [xu_parser](crates/xu_parser/) | Parsing (Token → AST) with error recovery |
| [xu_ir](crates/xu_ir/) | Intermediate representation (AST, Bytecode, Executable) |
| [xu_driver](crates/xu_driver/) | Frontend orchestration (lex → parse → analyze → compile) |
| [xu_runtime](crates/xu_runtime/) | Execution engine: AST interpreter and bytecode VM |
| [xu_cli](crates/xu_cli/) | Command-line interface (`xu` binary) |
| [xtask](crates/xtask/) | Development task runner |

### Compiler Pipeline

```
Source → xu_lexer → xu_parser → xu_driver → xu_runtime
           ↓           ↓           ↓            ↓
        Tokens       AST      Bytecode      Execution
```

## Testing

```bash
# Run all tests
cargo test

# Run spec tests only
cargo test -p xu_runtime --test runner

# Update golden files
XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner
```

## Benchmarks

Performance comparison with Python and Node.js:

```bash
# Run benchmarks (1M iterations)
python3 tests/benchmarks/py_node_report.py --runs 3 --scales 1000000
```

Results are saved to `tests/benchmarks/report.md`.

## Development

### xtask Commands

The project uses `xtask` for development automation:

```bash
# Run all quality checks (fmt + lint + test + examples)
cargo run -p xtask -- verify

# Individual commands
cargo run -p xtask -- fmt           # Check code formatting
cargo run -p xtask -- clippy        # Run clippy
cargo run -p xtask -- lint          # Strict lint (clippy::all + perf + nursery)
cargo run -p xtask -- test          # Run workspace tests
cargo run -p xtask -- examples      # Verify all examples
cargo run -p xtask -- codegen-examples  # Test JS/Python codegen
cargo run -p xtask -- check-unused  # Check unused dependencies (requires cargo-udeps)

# Performance
cargo run -p xtask -- perf                    # Run performance tests
cargo run -p xtask -- perf update-baseline    # Update performance baseline
cargo run -p xtask -- bench-report            # Generate benchmark report
cargo run -p xtask -- bench-report 1000000    # Benchmark with custom scale
```

### Quality Gate

Before committing, ensure all checks pass:

```bash
cargo run -p xtask -- verify
```

### CLI Commands

| Command | Description |
|---------|-------------|
| `xu run <file>` | Execute a script |
| `xu check <file>` | Syntax and type check |
| `xu ast <file>` | Print AST |
| `xu tokens <file>` | Print token stream |

## Documentation

Detailed documentation is available in the `docs/` directory (Chinese):

- Language Specification
- Standard Library Reference
- Grammar Definition
- Test Guide

## License

MIT

# xu_driver

Frontend orchestration layer for the XuScript compiler.

## Overview

This crate coordinates the compilation pipeline (normalize → lex → parse → analyze → compile) and provides a stable API for both CLI and runtime use. It also implements the `Frontend` trait for dynamic module loading.

## Architecture

```
xu_cli ──→ xu_driver ──→ xu_lexer
              ↓              ↓
xu_runtime ←─┘         xu_parser
                            ↓
                        xu_ir
```

## Key Components

| Component | File | Description |
|-----------|------|-------------|
| `Driver` | `frontend.rs` | Main facade for compilation |
| `analyze_module` | `analyzer/` | Static analysis and type checking |
| `compile_module` | `bytecode_compiler.rs` | AST to bytecode compilation |

## Usage

```rust
use xu_driver::Driver;

let driver = Driver::new();

// Parse only
let result = driver.parse_file("example.xu")?;

// Full compilation to executable
let compiled = driver.compile_file("example.xu", true)?;
// compiled.executable: Executable
// compiled.diagnostics: Vec<Diagnostic>
```

## Driver API

| Method | Description |
|--------|-------------|
| `lex_file` / `lex_text` | Tokenize source |
| `parse_file` / `parse_text` | Parse to AST |
| `compile_file` | Full compilation to `Executable` |

## Frontend Trait

The `xu_ir::Frontend` trait allows the runtime to compile modules dynamically during `use` statements:

```rust
impl Frontend for Driver {
    fn compile_text_no_analyze(&self, path: &str, text: &str)
        -> Result<CompiledUnit, String>;
}
```

This intentionally skips static analysis to avoid complex state dependencies during runtime module loading.

## Compilation Pipeline

```
SourceText
    ↓ normalize
NormalizedText
    ↓ lex
Vec<Token>
    ↓ parse
Module (AST)
    ↓ analyze
Module + Diagnostics
    ↓ compile
Executable::Bytecode(Program)
```

## Tests

Located in `crates/xu_driver/tests/`:
- Type system tests
- Static analysis behavior tests

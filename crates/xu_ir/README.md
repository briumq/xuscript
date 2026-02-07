# xu_ir

Intermediate representation shared between compiler stages.

## Overview

This crate defines the shared data structures used across the XuScript compiler:
- **AST**: Abstract Syntax Tree from parsing
- **Bytecode**: VM instructions from compilation
- **Executable**: Unified representation for runtime execution

## Architecture

```
xu_parser → xu_ir (AST)
               ↓
xu_driver → xu_ir (Bytecode)
               ↓
xu_runtime ← xu_ir (Executable)
```

## Key Types

| Type | File | Description |
|------|------|-------------|
| `Module`, `Stmt`, `Expr` | `ast.rs` | AST node types |
| `Bytecode`, `Op` | `bytecode.rs` | VM instruction set |
| `Program` | `program.rs` | Compiled module with bytecode |
| `Executable` | `executable.rs` | `Ast(Module)` or `Bytecode(Program)` |
| `Frontend` | `frontend.rs` | Pluggable compilation interface |

## Executable Dual-Path

The same source code can be executed via two paths:

```
SourceText
    ↓
Module (AST)
    ├──→ Executable::Ast ──→ AST Interpreter
    │
    └──→ Bytecode Compiler
              ↓
         Executable::Bytecode ──→ VM
```

### Why Two Paths?

| Path | Use Case |
|------|----------|
| AST | Debugging, semantic inspection, simpler implementation |
| Bytecode | Performance, controlled execution model, local variable slots |

## Usage

```rust
use xu_ir::{Executable, Module, Program};

// AST path
let executable = Executable::Ast(module);

// Bytecode path
let executable = Executable::Bytecode(program);

// Runtime executes either
runtime.exec_executable(&executable)?;
```

## Design Constraints

- AST and Bytecode semantics must remain equivalent
- `Executable` is the stable boundary between compiler and runtime
- New language features should be implemented in both paths

# xu_runtime

Runtime and virtual machine for XuScript execution.

## Overview

This crate provides the execution environment for XuScript programs:
- Module loading and caching
- Built-in functions and standard library
- Heap and environment management
- Dual execution paths: AST interpreter and bytecode VM

## Architecture

```
xu_cli
   ↓
xu_runtime::Runtime ←── xu_ir::Executable
   ├── Module Loader
   ├── Builtins Registry
   ├── AST Interpreter (ast_exec/)
   └── Bytecode VM (vm/)
```

## Key Components

| Directory | Description |
|-----------|-------------|
| `runtime/` | Core `Runtime` struct and execution logic |
| `ast_exec/` | AST interpreter implementation |
| `vm/` | Bytecode virtual machine |
| `builtins/` | Built-in function implementations |
| `methods/` | Type method implementations |
| `core/` | Value types, heap, and memory management |
| `modules/` | Module loading and resolution |

## Usage

```rust
use xu_runtime::Runtime;
use xu_ir::Executable;

let mut runtime = Runtime::new();

// Configure runtime
runtime.set_entry_path("example.xu")?;
runtime.set_frontend(Box::new(driver));

// Execute
let result = runtime.exec_executable(&executable)?;
println!("{}", result.output);
```

## Runtime API

| Method | Description |
|--------|-------------|
| `exec_executable` | Execute AST or bytecode |
| `set_frontend` | Inject compiler for dynamic imports |
| `set_entry_path` | Set script entry point |
| `set_stdlib_path` | Set standard library location |
| `set_args` | Set script arguments |

## Execution Model

### Entry Execution

1. Reset runtime state (output buffer, module cache, etc.)
2. Execute top-level statements
3. If `main` function exists, call it automatically

### Dual Execution Paths

```
Executable
    ├── Ast(Module) ──→ exec_module (AST interpreter)
    │
    └── Bytecode(Program) ──→ exec_program ──→ VM
```

## Module System

When a `use` statement is encountered:
1. Resolve path (relative to importing file)
2. Normalize and cache path
3. Detect circular imports (report with chain)
4. Compile and execute module
5. Return exports as dict

## Tests

Located in `crates/xu_runtime/tests/`:
- `runner.rs` - Unified test runner for specs/edge/integration
- Golden file tests
- Property-based tests
- Performance benchmarks

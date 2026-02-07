# xu_syntax

Core syntax and diagnostic types shared across all XuScript compiler crates.

## Overview

This crate provides the foundational types for source code representation, span tracking, tokenization, and diagnostic reporting. It serves as the stable base layer that all other compiler crates depend on.

## Architecture

```
xu_syntax
    ↓
┌───┴───┬───────┬──────────┬───────────┐
xu_lexer xu_parser xu_driver xu_runtime xu_cli
```

## Key Modules

| Module | Description |
|--------|-------------|
| `source.rs` | `SourceFile`, `SourceText` - source code representation |
| `span.rs` | `Span` - byte range location tracking |
| `token.rs` | `Token`, `TokenKind` - lexical token types |
| `diagnostic.rs` | `Diagnostic`, `DiagnosticKind`, `Severity` - structured errors |
| `render.rs` | `render_diagnostic` - human-readable diagnostic output |
| `builtins.rs` | Built-in function and type definitions |
| `types.rs` | Type system primitives |

## Usage

```rust
use xu_syntax::{SourceFile, Span, Diagnostic, DiagnosticKind};

// Create a source file
let source = SourceFile::new("example.xu", "let x = 42");

// Create a diagnostic
let diag = Diagnostic::error(
    DiagnosticKind::UnknownVariable("foo".into()),
    Span::new(0, 3),
);
```

## Design Notes

- Column numbers are character-based (Unicode-friendly)
- Diagnostics are structured - rendering is handled uniformly by `render_diagnostic`
- Supports both English and Chinese diagnostic messages

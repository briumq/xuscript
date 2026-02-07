# xu_parser

Syntax analysis (Token → AST) for XuScript.

## Overview

This crate parses token streams from `xu_lexer` into an Abstract Syntax Tree (`xu_ir::Module`), with error recovery to continue producing AST and diagnostics even when errors occur.

## Architecture

```
xu_lexer → xu_parser::Parser → xu_ir::Module + Vec<Diagnostic>
                                     ↓
                               xu_driver / xu_runtime
```

## Key Modules

| Module | Description |
|--------|-------------|
| `parser.rs` | `Parser::parse` - main parser entry point |
| `expr.rs` | Expression parsing (Pratt parser) |
| `stmt.rs` | Statement parsing |
| `types.rs` | Type annotation parsing |
| `interp.rs` | String interpolation handling |
| `utils.rs` | Parser utilities and helpers |

## Usage

```rust
use xu_parser::Parser;
use xu_lexer::Lexer;
use xu_syntax::SourceFile;

let source = SourceFile::new("example.xu", "let x = 42");
let (tokens, _) = Lexer::lex(&source);
let result = Parser::parse(&tokens, &source);
// result.module: Module AST
// result.diagnostics: Vec<Diagnostic>
```

## Parsing Strategy

### Pratt Expression Parsing

Operator precedence and associativity are handled uniformly using a Pratt parser, supporting:
- Binary operators (`+`, `-`, `*`, `/`, `==`, etc.)
- Unary operators (`-`, `!`)
- Member access (`.`)
- Function calls and indexing

### Error Recovery

The parser uses synchronization points to recover from errors:
- Statement terminators
- Newlines
- `DEDENT` tokens
- Block boundaries

This prevents a single error from cascading through the entire file.

### Interpolation Caching

String interpolation expressions are cached to reduce redundant parsing.

## Tests

Located in `crates/xu_parser/tests/`:
- Golden snapshots (tokens, AST, diagnostics)
- Property-based fuzzing
- Import sugar
- Interpolation edge cases

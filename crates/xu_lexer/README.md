# xu_lexer

Lexical analysis and source normalization for XuScript.

## Overview

This crate transforms source text into a stream of tokens, handling:
- Source normalization (line endings, encoding)
- Indentation-based block structure (`INDENT`/`DEDENT` tokens)
- Keyword and punctuation recognition
- Basic lexical diagnostics

## Architecture

```
SourceText → xu_lexer::Lexer → Vec<Token> + Vec<Diagnostic>
                                    ↓
                              xu_parser
```

## Key Modules

| Module | Description |
|--------|-------------|
| `lexer.rs` | `Lexer::lex` - main lexer entry point |
| `normalize.rs` | `normalize_source` - source text normalization |
| `keywords.rs` | Keyword definitions and lookup |

## Usage

```rust
use xu_lexer::Lexer;
use xu_syntax::SourceFile;

let source = SourceFile::new("example.xu", "let x = 42");
let (tokens, diagnostics) = Lexer::lex(&source);
```

## Lexer Behavior

### Indentation Handling

The lexer tracks indentation levels using a stack:
- Increased indentation emits `INDENT`
- Decreased indentation emits `DEDENT`
- Inside brackets/parentheses, indentation is suppressed

### Delimiter Depth

Bracket depth tracking prevents multi-line expressions from being misinterpreted as block structures:

```xu
let list = [
    1,
    2,  // No INDENT/DEDENT here
    3,
]
```

## Tests

Located in `crates/xu_lexer/tests/`:
- Smoke tests
- Punctuation rules
- Property-based fuzzing

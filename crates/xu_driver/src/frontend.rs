//!
//!
//!
//!

use std::fs;
use std::sync::{Arc, RwLock};

use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_syntax::{Diagnostic, SourceFile, SourceId, Token};

use crate::analyzer::{ImportCache, analyze_module};
use crate::bytecode_compiler;

pub struct Driver {
    pub cache: Arc<RwLock<ImportCache>>,
}

impl xu_ir::Frontend for Driver {
    fn compile_text_no_analyze(
        &self,
        path: &str,
        input: &str,
    ) -> Result<xu_ir::CompiledUnit, String> {
        let parsed = self.parse_text_no_analyze(path, input)?;
        let bc = bytecode_compiler::compile_module(&parsed.module);
        let executable = xu_ir::Executable::Bytecode(xu_ir::Program {
            module: parsed.module.clone(),
            bytecode: bc,
        });
        Ok(xu_ir::CompiledUnit {
            text: parsed.source.text.as_str().to_string(),
            executable,
            diagnostics: parsed.diagnostics,
        })
    }
}

#[derive(Clone, Debug)]
pub struct CompiledFile {
    pub path: String,
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub executable: xu_ir::Executable,
    pub diagnostics: Vec<Diagnostic>,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(ImportCache::default())),
        }
    }

    pub fn lex_file(&self, path: &str) -> Result<LexedFile, String> {
        let input =
            fs::read_to_string(path).map_err(|e| format!("Failed to read file {path}: {e}"))?;
        self.lex_text(path, &input)
    }

    pub fn lex_text(&self, path: &str, input: &str) -> Result<LexedFile, String> {
        let normalized = normalize_source(input);
        let source = SourceFile::new(SourceId(0), path.to_string(), normalized.text);
        let lex = Lexer::new(source.text.as_str()).lex();

        Ok(LexedFile {
            path: path.to_string(),
            source,
            tokens: lex.tokens,
            diagnostics: lex.diagnostics,
        })
    }

    fn lex_parse_inner(
        &self,
        path: &str,
        input: &str,
    ) -> Result<
        (
            SourceFile,
            Vec<Token>,
            xu_ir::Module,
            Vec<Diagnostic>,
            Timings,
        ),
        String,
    > {
        let t1 = std::time::Instant::now();
        let normalized = normalize_source(input);
        let t2 = std::time::Instant::now();
        let source = SourceFile::new(SourceId(0), path.to_string(), normalized.text);
        let lex = Lexer::new(source.text.as_str()).lex();
        let t3 = std::time::Instant::now();
        let bump = bumpalo::Bump::new();
        let parse = Parser::new(source.text.as_str(), &lex.tokens, &bump).parse();
        let t4 = std::time::Instant::now();

        let mut diagnostics = lex.diagnostics;
        diagnostics.extend(parse.diagnostics);

        Ok((
            source,
            lex.tokens,
            parse.module,
            diagnostics,
            Timings {
                normalize_us: (t2 - t1).as_micros(),
                lex_us: (t3 - t2).as_micros(),
                parse_us: (t4 - t3).as_micros(),
                analyze_us: 0,
            },
        ))
    }

    pub fn parse_file(&self, path: &str, strict: bool) -> Result<ParsedFile, String> {
        let input =
            fs::read_to_string(path).map_err(|e| format!("Failed to read file {path}: {e}"))?;
        self.parse_text(path, &input, strict)
    }

    pub fn compile_file(&self, path: &str, strict: bool) -> Result<CompiledFile, String> {
        let ParsedFile {
            path,
            source,
            tokens,
            module,
            diagnostics,
        } = self.parse_file(path.as_ref(), strict)?;
        let bc = bytecode_compiler::compile_module(&module);
        Ok(CompiledFile {
            path,
            source,
            tokens,
            executable: xu_ir::Executable::Bytecode(xu_ir::Program {
                module,
                bytecode: bc,
            }),
            diagnostics,
        })
    }

    pub fn parse_text_no_analyze(&self, path: &str, input: &str) -> Result<ParsedFile, String> {
        let (source, tokens, module, diagnostics, _tm) = self.lex_parse_inner(path, input)?;

        Ok(ParsedFile {
            path: path.to_string(),
            source,
            tokens,
            module,
            diagnostics,
        })
    }

    ///
    ///
    ///
    ///
    ///
    ///
    pub fn parse_text(&self, path: &str, input: &str, strict: bool) -> Result<ParsedFile, String> {
        let (source, tokens, mut module, mut diagnostics, _tm) =
            self.lex_parse_inner(path, input)?;
        let mut import_stack = Vec::new();
        diagnostics.extend(analyze_module(
            &source,
            &tokens,
            &mut module,
            strict,
            self.cache.clone(),
            &mut import_stack,
        ));

        Ok(ParsedFile {
            path: path.to_string(),
            source,
            tokens,
            module,
            diagnostics,
        })
    }

    pub fn parse_text_timed(
        &self,
        path: &str,
        input: &str,
        strict: bool,
    ) -> Result<(ParsedFile, Timings), String> {
        let (source, tokens, mut module, mut diagnostics, mut tm) =
            self.lex_parse_inner(path, input)?;
        let mut import_stack = Vec::new();
        let t4 = std::time::Instant::now();
        let analysis = analyze_module(
            &source,
            &tokens,
            &mut module,
            strict,
            self.cache.clone(),
            &mut import_stack,
        );
        let t5 = std::time::Instant::now();
        diagnostics.extend(analysis);

        let pf = ParsedFile {
            path: path.to_string(),
            source,
            tokens,
            module,
            diagnostics,
        };
        tm.analyze_us = (t5 - t4).as_micros();
        Ok((pf, tm))
    }
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LexedFile {
    pub path: String,
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct ParsedFile {
    pub path: String,
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub module: xu_ir::Module,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug)]
pub struct Timings {
    pub normalize_us: u128,
    pub lex_us: u128,
    pub parse_us: u128,
    pub analyze_us: u128,
}

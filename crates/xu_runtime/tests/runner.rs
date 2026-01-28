use std::fs;
use std::path::PathBuf;
use std::thread;

use xu_driver::Driver;
#[allow(unused_imports)]
use xu_ir::{Executable, Frontend};
use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

mod common;
use common::{assert_or_update, find_files, golden_path_for, repo_root};

#[derive(Clone)]
enum Strategy {
    /// Run source, capture output, compare with golden file.
    /// If inject_root is true, defines __ROOT__ global.
    RunAndCompare {
        golden_subdir: &'static str,
        inject_root: bool,
    },
    /// Compile to AST and VM, run both, compare outputs.
    /// Used for verification of VM correctness.
    AstVsVm,
    /// Just run the source and expect success.
    /// Used for specs that use assertions internally.
    RunOnly,
}

struct Suite {
    name: &'static str,
    root: &'static str,
    pattern: &'static str,
    strategy: Strategy,
}

#[test]
fn main_runner() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let suites = vec![
                Suite {
                    name: "specs",
                    root: "tests/specs",
                    pattern: "xu",
                    strategy: Strategy::RunOnly,
                },
                Suite {
                    name: "edge",
                    root: "tests/edge",
                    pattern: "xu",
                    strategy: Strategy::AstVsVm,
                },
                Suite {
                    name: "integration",
                    root: "tests/integration",
                    pattern: "xu",
                    strategy: Strategy::RunAndCompare {
                        golden_subdir: "integration",
                        inject_root: true,
                    },
                },
                Suite {
                    name: "integration_en",
                    root: "tests/integration_en",
                    pattern: "xu",
                    strategy: Strategy::RunAndCompare {
                        golden_subdir: "integration_en",
                        inject_root: true,
                    },
                },
            ];

            let filter = std::env::var("XU_TEST_ONLY").ok();

            for suite in suites {
                let root_path = repo_root().join(suite.root);
                if !root_path.exists() {
                    eprintln!("Suite {} root not found: {:?}", suite.name, root_path);
                    continue;
                }
                let files = find_files(&root_path, suite.pattern);

                for path in files {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let suite_name = suite.name;

                    if let Some(f) = &filter {
                        if !name.contains(f) && !suite_name.contains(f) {
                            continue;
                        }
                    }

                    // Special exclusion for specs suite to avoid overlap with typecheck
                    if suite_name == "specs" && path.to_string_lossy().contains("typecheck") {
                        continue;
                    }
                    if suite_name == "specs" && name != "assert_basic" {
                        continue;
                    }

                    let strategy = suite.strategy.clone();
                    let suite_root = suite.root;

                    run_test(suite_name, suite_root, &path, strategy);
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

fn run_test(suite_name: &str, _suite_root: &str, path: &PathBuf, strategy: Strategy) {
    eprintln!(
        "Running {}/{}",
        suite_name,
        path.file_name().unwrap().to_string_lossy()
    );
    match strategy {
        Strategy::RunOnly => {
            let src = fs::read_to_string(path).expect("read source");
            if let Err(e) = run_spec_file(path, &src) {
                panic!("Spec failed {}: {}", path.display(), e);
            }
        }
        Strategy::RunAndCompare {
            golden_subdir,
            inject_root,
        } => {
            let src = fs::read_to_string(path).expect("read source");

            let normalized = normalize_source(&src);
            assert!(
                normalized.diagnostics.is_empty(),
                "Normalize errors: {:?}",
                normalized.diagnostics
            );

            let lex = Lexer::new(&normalized.text).lex();
            assert!(
                lex.diagnostics.is_empty(),
                "Lex errors: {:?}",
                lex.diagnostics
            );

            let bump = bumpalo::Bump::new();
            let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
            let errors: Vec<_> = parse
                .diagnostics
                .iter()
                .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
                .collect();
            assert!(errors.is_empty(), "Parse errors: {:?}", errors);

            let mut rt = Runtime::new();
            rt.set_rng_seed(1); // Deterministic
            rt.set_frontend(Box::new(xu_driver::Driver::new()));
            rt.set_entry_path(&path.to_string_lossy()).unwrap();

            let root = repo_root();
            let stdlib_path = root.join("stdlib");
            rt.set_stdlib_path(stdlib_path.to_string_lossy().to_string());

            if inject_root {
                eprintln!("Injecting __ROOT__ = {}", root.to_string_lossy());
                rt.define_global_constant("__ROOT__", &root.to_string_lossy());
            }

            // Set CWD to repo root for consistent paths
            let old_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(&root).unwrap();

            let result = rt.exec_module(&parse.module);

            std::env::set_current_dir(old_cwd).unwrap();

            let result =
                result.unwrap_or_else(|e| panic!("Runtime error in {}: {}", path.display(), e));

            let name = path.file_stem().unwrap().to_string_lossy();
            let subdir = golden_subdir;
            assert_or_update(golden_path_for(subdir, &name), &result.output);
        }
        Strategy::AstVsVm => {
            let src = fs::read_to_string(path).expect("read source");
            let root = repo_root();
            let old_cwd = std::env::current_dir().unwrap();
            eprintln!("AstVsVm setting CWD to {:?}", root);
            std::env::set_current_dir(&root).unwrap();

            let compiled = Driver::new()
                .compile_text_no_analyze(path.to_string_lossy().as_ref(), &src)
                .unwrap();

            if let Executable::Bytecode(program) = compiled.executable {
                if program.bytecode.is_some() {
                    // Run VM
                    let mut rt_vm = Runtime::new();
                    setup_rt_for_edge(&mut rt_vm, path);

                    let out_vm = rt_vm
                        .exec_executable(&Executable::Bytecode(program.clone()))
                        .unwrap_or_else(|e| {
                            let out = rt_vm.take_output();
                            panic!("VM failed for {}: {}\nOutput: {}", path.display(), e, out);
                        })
                        .output;

                    // Run AST
                    let mut rt_ast = Runtime::new();
                    setup_rt_for_edge(&mut rt_ast, path);

                    let out_ast = rt_ast
                        .exec_module(&program.module)
                        .unwrap_or_else(|e| {
                            let out = rt_ast.take_output();
                            panic!("AST failed for {}: {}\nOutput: {}", path.display(), e, out);
                        })
                        .output;

                    assert_eq!(
                        out_vm,
                        out_ast,
                        "AST vs VM output mismatch for {}",
                        path.display()
                    );
                } else {
                    // No bytecode generated (maybe empty?), just run AST
                    let mut rt_ast = Runtime::new();
                    setup_rt_for_edge(&mut rt_ast, path);
                    rt_ast.exec_module(&program.module).unwrap();
                }
            } else {
                panic!("Expected Bytecode executable for {}", path.display());
            }
            std::env::set_current_dir(old_cwd).unwrap();
        }
    }
}

#[allow(dead_code)]
fn run_spec_file(path: &PathBuf, src: &str) -> Result<String, String> {
    let normalized = normalize_source(src);
    if !normalized.diagnostics.is_empty() {
        return Err(format!(
            "normalize diagnostics: {:?}",
            normalized.diagnostics
        ));
    }
    let parse = {
        let lex = Lexer::new(&normalized.text).lex();
        if !lex.diagnostics.is_empty() {
            return Err(format!("lex diagnostics: {:?}", lex.diagnostics));
        }
        let bump = bumpalo::Bump::new();
        let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
        let errors: Vec<_> = parse
            .diagnostics
            .iter()
            .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
            .collect();
        if !errors.is_empty() {
            return Err(format!("parse errors: {:?}", errors));
        }
        parse
    };

    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_entry_path(&path.to_string_lossy()).unwrap();
    let stdlib_path = repo_root().join("stdlib");
    rt.set_stdlib_path(stdlib_path.to_string_lossy().to_string());

    // Set CWD to file's parent directory to allow relative imports
    let old_cwd = std::env::current_dir().unwrap();
    if let Some(parent) = path.parent() {
        std::env::set_current_dir(parent).unwrap();
    }

    let result = rt.exec_module(&parse.module);
    std::env::set_current_dir(old_cwd).unwrap();

    result.map(|r| r.output).map_err(|e| e.to_string())
}

#[allow(dead_code)]
fn setup_rt_for_edge(rt: &mut Runtime, entry_path: &PathBuf) {
    let root = repo_root();
    let stdlib_path = root.join("stdlib");
    let tests_root = root.join("tests");
    let edge_dir = root.join("tests/edge");

    let _ = rt.add_allowed_root(&edge_dir.to_string_lossy());
    let _ = rt.add_allowed_root(&tests_root.to_string_lossy());
    let _ = rt.add_allowed_root(&stdlib_path.to_string_lossy());
    rt.set_stdlib_path(stdlib_path.to_string_lossy().to_string());
    rt.set_frontend(Box::new(Driver::new()));
    rt.set_entry_path(&entry_path.to_string_lossy()).unwrap();
    rt.set_rng_seed(1);
}

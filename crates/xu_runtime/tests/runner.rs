use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::thread;

use serde_json::Value;
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

#[derive(Default, Clone)]
struct ExamplesManifest {
    check_skip_prefixes: Vec<String>,
    run_expect_fail: Vec<String>,
}

fn parse_csv_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn env_bool(name: &str) -> Option<bool> {
    match std::env::var(name).ok().as_deref() {
        None => None,
        Some("0") | Some("false") => Some(false),
        Some("1") | Some("true") => Some(true),
        Some(_) => None,
    }
}

fn load_examples_manifest(examples_root: &Path) -> ExamplesManifest {
    let path = examples_root.join("manifest.json");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return ExamplesManifest::default(),
    };
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse examples manifest {}: {}", path.display(), e);
            return ExamplesManifest::default();
        }
    };

    let mut out = ExamplesManifest::default();
    if let Some(arr) = v.get("check_skip_prefixes").and_then(|v| v.as_array()) {
        for it in arr {
            if let Some(s) = it.as_str() {
                out.check_skip_prefixes.push(s.to_string());
            }
        }
    }
    if let Some(arr) = v.get("run_expect_fail").and_then(|v| v.as_array()) {
        for it in arr {
            if let Some(s) = it.as_str() {
                out.run_expect_fail.push(s.to_string());
            }
        }
    }
    out
}

fn path_contains_any(p: &Path, needles: &[String]) -> bool {
    if needles.is_empty() {
        return false;
    }
    let s = p.to_string_lossy();
    needles.iter().any(|n| !n.is_empty() && s.contains(n))
}

fn example_rel_string(examples_root: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(examples_root).unwrap_or(p);
    rel.to_string_lossy().replace('\\', "/")
}

fn example_matches_token(rel: &str, stem: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if rel.starts_with(token) {
        return true;
    }
    if stem == token {
        return true;
    }
    if token.ends_with(".xu") && (rel == token || rel.ends_with(&format!("/{token}"))) {
        return true;
    }
    false
}

fn example_matches_any(rel: &str, stem: &str, tokens: &[String]) -> bool {
    tokens
        .iter()
        .any(|t| example_matches_token(rel, stem, t.trim()))
}

#[test]
fn main_runner() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let mut suites = vec![
                Suite {
                    name: "specs",
                    root: "tests/specs",
                    pattern: "xu",
                    strategy: Strategy::RunOnly,
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
            if std::env::var("XU_TEST_DRAFTS").ok().as_deref() == Some("1") {
                suites.push(Suite {
                    name: "specs_drafts",
                    root: "tests/specs/v1_1_drafts",
                    pattern: "xu",
                    strategy: Strategy::RunOnly,
                });
            }
            let edge_enabled = match std::env::var("XU_TEST_EDGE").ok().as_deref() {
                None => true,
                Some("0") | Some("false") => false,
                Some("1") | Some("true") => true,
                Some(_) => true,
            };
            if edge_enabled {
                suites.push(Suite {
                    name: "edge",
                    root: "tests/edge",
                    pattern: "xu",
                    strategy: Strategy::AstVsVm,
                });
            }
            let examples_enabled = match std::env::var("XU_TEST_EXAMPLES").ok().as_deref() {
                None => true,
                Some("0") | Some("false") => false,
                Some("1") | Some("true") => true,
                Some(_) => true,
            };
            if examples_enabled {
                suites.push(Suite {
                    name: "examples",
                    root: "examples",
                    pattern: "xu",
                    strategy: Strategy::RunAndCompare {
                        golden_subdir: "examples",
                        inject_root: true,
                    },
                });
            }

            let filter = std::env::var("XU_TEST_ONLY").ok();
            let skip = std::env::var("XU_TEST_SKIP")
                .ok()
                .map(|s| parse_csv_list(&s))
                .unwrap_or_default();
            let include_examples_expect_fail =
                env_bool("XU_TEST_EXAMPLES_INCLUDE_EXPECT_FAIL").unwrap_or(false);

            for suite in suites {
                let root_path = repo_root().join(suite.root);
                if !root_path.exists() {
                    eprintln!("Suite {} root not found: {:?}", suite.name, root_path);
                    continue;
                }
                let mut files = find_files(&root_path, suite.pattern);
                if suite.name == "examples" {
                    files.retain(|p| !p.to_string_lossy().contains("/modules/"));
                    let manifest = load_examples_manifest(&root_path);
                    files.retain(|p| {
                        let rel = example_rel_string(&root_path, p);
                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if path_contains_any(p, &skip) {
                            return false;
                        }
                        if example_matches_any(&rel, stem, &manifest.check_skip_prefixes) {
                            return false;
                        }
                        if !include_examples_expect_fail
                            && example_matches_any(&rel, stem, &manifest.run_expect_fail)
                        {
                            return false;
                        }
                        true
                    });
                } else if !skip.is_empty() {
                    files.retain(|p| !path_contains_any(p, &skip));
                }

                for path in files {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    let suite_name = suite.name;

                    if let Some(f) = &filter {
                        if !name.contains(f) && !suite_name.contains(f) {
                            continue;
                        }
                    }

                    if suite_name == "specs" && path.to_string_lossy().contains("typecheck") {
                        continue;
                    }
                    if suite_name == "specs" && path.to_string_lossy().contains("/v1_1_drafts/") {
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
            match parse_expectation(&src) {
                Some(Expectation::Skip { reason }) => {
                    eprintln!("  Skipped: {}", reason);
                    return;
                }
                Some(Expectation::ExpectError { contains }) => {
                    let driver = Driver::new();
                    let parsed = driver
                        .parse_text(path.to_string_lossy().as_ref(), &src, true)
                        .unwrap();
                    let errs: Vec<_> = parsed
                        .diagnostics
                        .into_iter()
                        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
                        .collect();
                    if errs.is_empty() {
                        panic!("Expected error, got none: {}", path.display());
                    }
                    let joined = errs
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !joined.contains(&contains) {
                        panic!("Expected error containing {:?}, got:\n{}", contains, joined);
                    }
                }
                Some(Expectation::ExpectWarn { contains }) => {
                    let driver = Driver::new();
                    let parsed = driver
                        .parse_text(path.to_string_lossy().as_ref(), &src, true)
                        .unwrap();
                    let warns: Vec<_> = parsed
                        .diagnostics
                        .into_iter()
                        .filter(|d| matches!(d.severity, xu_syntax::Severity::Warning))
                        .collect();
                    if warns.is_empty() {
                        panic!("Expected warning, got none: {}", path.display());
                    }
                    let joined = warns
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !joined.contains(&contains) {
                        panic!("Expected warning containing {:?}, got:\n{}", contains, joined);
                    }
                }
                Some(Expectation::ExpectPanic { contains }) => match run_spec_file(path, &src) {
                    Ok(_) => panic!("Expected runtime failure, got success: {}", path.display()),
                    Err(e) => {
                        if !e.contains(&contains) {
                            panic!(
                                "Expected runtime failure containing {:?}, got:\n{}",
                                contains, e
                            );
                        }
                    }
                },
                None => {
                    if let Err(e) = run_spec_file(path, &src) {
                        panic!("Spec failed {}: {}", path.display(), e);
                    }
                }
            }
        }
        Strategy::RunAndCompare {
            golden_subdir,
            inject_root,
        } => {
            let src = fs::read_to_string(path).expect("read source");
            let driver = Driver::new();
            let root_predefs = ["__ROOT__"];
            let extra_predefs: &[&str] = if inject_root { &root_predefs } else { &[] };
            let parsed = driver
                .parse_text_with_predefs(path.to_string_lossy().as_ref(), &src, true, extra_predefs)
                .unwrap();
            let errors: Vec<_> = parsed
                .diagnostics
                .iter()
                .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
                .collect();
            assert!(errors.is_empty(), "Diagnostics errors: {:?}", errors);

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

            let result = rt.exec_module(&parsed.module);

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

#[derive(Debug, Clone)]
enum Expectation {
    ExpectError { contains: String },
    ExpectWarn { contains: String },
    ExpectPanic { contains: String },
    Skip { reason: String },
}

fn parse_expectation(src: &str) -> Option<Expectation> {
    for line in src.lines() {
        // Check for skip first
        if line.contains("// skip:") {
            let reason = line.split("// skip:").nth(1).unwrap_or("").trim().to_string();
            return Some(Expectation::Skip { reason });
        }
        if let Some(s) = parse_expectation_from_line(line, "expect_error:") {
            return Some(Expectation::ExpectError { contains: s });
        }
        if let Some(s) = parse_expectation_from_line(line, "expect_warn:") {
            return Some(Expectation::ExpectWarn { contains: s });
        }
        if let Some(s) = parse_expectation_from_line(line, "expect_panic:") {
            return Some(Expectation::ExpectPanic { contains: s });
        }
    }
    None
}

fn parse_expectation_from_line(line: &str, key: &str) -> Option<String> {
    let i = line.find(key)?;
    let rest = &line[i + key.len()..];
    let q0 = rest.find('"')?;
    let rest2 = &rest[q0 + 1..];
    let q1 = rest2.find('"')?;
    Some(rest2[..q1].to_string())
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

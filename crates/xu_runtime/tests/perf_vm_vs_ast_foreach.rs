use std::time::Instant;
use xu_driver::Driver;
use xu_ir::{Executable, Frontend};
use xu_runtime::Runtime;

#[test]
#[ignore]
fn perf_vm_vs_ast_foreach_range_hotloop() {
    // Small foreach + output program to exercise both bytecode generation and VM execution.
    let src = "for n in [1..32]:\n    println(\"{n}\");\n";
    let compiled = Driver::new()
        .compile_text_no_analyze("<bench_foreach>", src)
        .unwrap();
    let Executable::Bytecode(program) = compiled.executable else {
        panic!("expected bytecode program");
    };
    if program.bytecode.is_none() {
        panic!("{:#?}", program.module);
    }
    let bytecode_exec = Executable::Bytecode(program.clone());
    let module = program.module.clone();

    let iters: usize = 50_000;

    // AST execution
    let mut rt_ast = Runtime::new();
    rt_ast.set_frontend(Box::new(Driver::new()));
    let warm = rt_ast.exec_module(&module).unwrap().output;
    assert!(!warm.is_empty());
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = rt_ast.exec_module(&module).unwrap();
    }
    let t1 = Instant::now();

    // VM execution
    let mut rt_vm = Runtime::new();
    let warm = rt_vm.exec_executable(&bytecode_exec).unwrap().output;
    assert!(!warm.is_empty());
    let t2 = Instant::now();
    for _ in 0..iters {
        let _ = rt_vm.exec_executable(&bytecode_exec).unwrap();
    }
    let t3 = Instant::now();

    let ast_ms = (t1 - t0).as_millis();
    let vm_ms = (t3 - t2).as_millis();
    println!(
        "PERF|perf_vm_vs_ast_foreach_range_hotloop|iters={iters}|ast_ms={ast_ms}|vm_ms={vm_ms}"
    );
}

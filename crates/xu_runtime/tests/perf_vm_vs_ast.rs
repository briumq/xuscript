use std::time::Instant;

use xu_driver::Driver;
use xu_ir::Executable;
use xu_ir::Frontend;
use xu_runtime::Runtime;

#[test]
#[ignore]
fn perf_vm_vs_ast_15_dict_merge_hotloop() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/specs/15_dict_merge.xu");
    let src = std::fs::read_to_string(&path).unwrap();
    let compiled = Driver::new()
        .compile_text_no_analyze(path.to_string_lossy().as_ref(), &src)
        .unwrap();
    let Executable::Bytecode(program) = compiled.executable else {
        panic!("expected bytecode program");
    };
    let bytecode_exec = Executable::Bytecode(program.clone());
    let module = program.module.clone();

    let iters: usize = 200_000;

    let mut rt_ast = Runtime::new();
    rt_ast.set_frontend(Box::new(Driver::new()));
    rt_ast
        .set_entry_path(path.to_string_lossy().as_ref())
        .unwrap();
    let warm = rt_ast.exec_module(&module).unwrap().output;
    assert!(!warm.is_empty());
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = rt_ast.exec_module(&module).unwrap();
    }
    let t1 = Instant::now();

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
        "PERF|perf_vm_vs_ast_15_dict_merge_hotloop|iters={iters}|ast_ms={ast_ms}|vm_ms={vm_ms}"
    );
}

use xu_driver::Driver;
use xu_ir::Frontend;
use xu_ir::{Bytecode, Constant, Executable, Op, Program};
use xu_runtime::Runtime;

#[test]
fn runtime_executes_bytecode_when_present() {
    let bc = Bytecode {
        ops: vec![Op::Const(0), Op::Print, Op::Halt],
        constants: vec![Constant::Str("X".to_string())],
    };
    let program = Program {
        module: xu_ir::Module {
            stmts: vec![].into(),
        },
        bytecode: Some(bc),
    };
    let executable = Executable::Bytecode(program);

    let mut rt = Runtime::new();
    let res = rt.exec_executable(&executable).unwrap();
    assert_eq!(res.output, "X\n");
}

#[test]
fn driver_emits_bytecode_for_supported_subset() {
    let src = "println(1 + 2);";
    let compiled = Driver::new()
        .compile_text_no_analyze("<test>", src)
        .unwrap();

    let Executable::Bytecode(program) = compiled.executable else {
        panic!("expected bytecode program");
    };
    if program.bytecode.is_none() {
        panic!("{:#?}", program.module);
    }

    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(Driver::new()));
    let res = rt.exec_executable(&Executable::Bytecode(program)).unwrap();
    assert_eq!(res.output, "3\n");
}

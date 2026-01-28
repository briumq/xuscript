use xu_ir::Frontend;
use xu_runtime::Runtime;

#[test]
fn driver_compiles_while_and_add_assign_to_vm() {
    let src = r#"
i = 0;
while i < 3 {
    println(i);
    i += 1;
}
"#;
    let driver = xu_driver::Driver::new();
    let cu = driver
        .compile_text_no_analyze("<mem>", src)
        .expect("compile");
    let mut rt = Runtime::new();
    let res = rt.exec_executable(&cu.executable).expect("exec");
    assert_eq!(res.output.trim_end(), "0\n1\n2");
}

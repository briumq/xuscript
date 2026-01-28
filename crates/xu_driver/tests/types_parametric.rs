use xu_driver::Driver;

fn collect_errors(src: &str) -> Vec<String> {
    let driver = Driver::new();
    let parsed = driver.parse_text("<test>", src, true).unwrap();
    parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
}

#[test]
fn typed_list_int_ok() {
    let errs = collect_errors("x: list[int] = [1, 2, 3];\n");
    assert!(
        errs.iter().all(|e| !e.contains("Type mismatch")),
        "{:?}",
        errs
    );
}

#[test]
fn typed_list_int_mismatch() {
    let errs = collect_errors("x: list[int] = [1, \"a\"];\n");
    assert!(
        errs.iter().any(|e| e.contains("Type mismatch")),
        "{:?}",
        errs
    );
}

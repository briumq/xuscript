use xu_runtime::Runtime;

#[test]
fn builder_names_exist() {
    let rt = Runtime::new();
    assert!(rt.has_builtin("builder_new"));
    assert!(rt.has_builtin("builder_new_cap"));
    assert!(rt.has_builtin("builder_push"));
    assert!(rt.has_builtin("builder_finalize"));
}

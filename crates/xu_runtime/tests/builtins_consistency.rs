use xu_runtime::runtime::builtins_registry::{
    BuiltinProvider, BuiltinRegistry, StdBuiltinProvider,
};
use xu_syntax::BUILTIN_NAMES;

#[test]
fn builtins_registry_matches_syntax_list() {
    let mut reg = BuiltinRegistry::new();
    StdBuiltinProvider.install(&mut reg);
    let mut a = reg.names();
    let mut b = BUILTIN_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

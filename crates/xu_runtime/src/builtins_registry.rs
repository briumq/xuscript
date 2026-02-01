use crate::Value;
use crate::core::value::Function;

use super::Runtime;
use super::builtins;
use crate::core::env::Env;

pub type BuiltinFn = fn(&mut Runtime, &[Value]) -> Result<Value, String>;

pub struct BuiltinRegistry {
    entries: Vec<(String, BuiltinFn)>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn register(&mut self, name: &str, fun: BuiltinFn) {
        self.entries.push((name.to_string(), fun));
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|(n, _)| n.clone()).collect()
    }

    pub fn install_into(self, env: &mut Env, heap: &mut crate::core::gc::Heap) {
        for (name, fun) in self.entries {
            let id = heap.alloc(crate::core::gc::ManagedObject::Function(Function::Builtin(fun)));
            env.define(name, Value::function(id));
        }
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub trait BuiltinProvider {
    fn install(&self, registry: &mut BuiltinRegistry);
}

pub struct StdBuiltinProvider;

impl BuiltinProvider for StdBuiltinProvider {
    fn install(&self, registry: &mut BuiltinRegistry) {
        registry.register("print", builtins::builtin_print);
        registry.register("println", builtins::builtin_print);
        registry.register("gen_id", builtins::builtin_gen_id);
        registry.register("gc", builtins::builtin_gc);
        registry.register("open", builtins::builtin_open);
        registry.register("input", builtins::builtin_input);
        registry.register("time_unix", builtins::builtin_time_unix);
        registry.register("time_millis", builtins::builtin_time_millis);
        registry.register("mono_micros", builtins::builtin_mono_micros);
        registry.register("mono_nanos", builtins::builtin_mono_nanos);
        registry.register("abs", builtins::builtin_abs);
        registry.register("max", builtins::builtin_max);
        registry.register("min", builtins::builtin_min);
        registry.register("rand", builtins::builtin_rand);
        registry.register("to_text", builtins::builtin_to_text);
        registry.register("parse_int", builtins::builtin_parse_int);
        registry.register("parse_float", builtins::builtin_parse_float);
        // builder
        registry.register(
            "builder_new_cap",
            builtins::builtin_builder_new_with_capacity,
        );
        registry.register("builder_new", builtins::builtin_builder_new);
        registry.register("builder_push", builtins::builtin_builder_push);
        registry.register("builder_finalize", builtins::builtin_builder_finalize);
        registry.register("os_args", builtins::builtin_os_args);
        // string helpers
        registry.register("contains", builtins::builtin_contains);
        registry.register("starts_with", builtins::builtin_starts_with);
        registry.register("ends_with", builtins::builtin_ends_with);
        registry.register("process_rss", builtins::builtin_process_rss);
        registry.register("sin", builtins::builtin_sin);
        registry.register("cos", builtins::builtin_cos);
        registry.register("tan", builtins::builtin_tan);
        registry.register("sqrt", builtins::builtin_sqrt);
        registry.register("log", builtins::builtin_log);
        registry.register("pow", builtins::builtin_pow);
        registry.register("__builtin_assert", builtins::builtin_assert);
        registry.register("__builtin_assert_eq", builtins::builtin_assert_eq);
        registry.register("__set_from_list", builtins::builtin_set_from_list);
        registry.register("__heap_stats", builtins::builtin_heap_stats);
    }
}

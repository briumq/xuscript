use std::collections::HashSet;
use std::hash::{BuildHasher, Hash, Hasher};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use hashbrown::hash_map::RawEntryMut;
use smallvec::SmallVec;
use xu_ir::{BinaryOp, Executable, Expr, Module, Stmt, StructDef};

use crate::Value;
use crate::Env;
use crate::value::{Dict, DictKey, FastHashMap, fast_map_new};
use crate::capabilities;
use crate::module_loader;
use crate::modules;
use crate::slot_allocator;
use crate::ir;
use crate::builtins_registry;
use crate::methods;

type HashMap<K, V> = FastHashMap<K, V>;

// Re-exports
pub use crate::text::Text;

// These types are defined in this file, no need to re-export from crate

pub(crate) use crate::methods::MethodKind;
use crate::util::value_to_string;

#[derive(Debug)]
pub struct ExecResult {
    pub value: Option<Value>,
    pub output: String,
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    pub strict_vars: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { strict_vars: true }
    }
}

#[derive(Clone, Default)]
pub struct MethodICSlot {
    pub tag: u64,
    pub method_hash: u64,
    pub struct_ty_hash: u64,
    pub(crate) kind: MethodKind,
    pub cached_func: Value,
    pub cached_user: Option<std::rc::Rc<crate::value::UserFunction>>,
    pub cached_bytecode: Option<std::rc::Rc<crate::value::BytecodeFunction>>,
}

pub struct Runtime {
    pub(crate) env: Env,
    pub(crate) env_pool: Vec<Env>,
    pub(crate) heap: crate::gc::Heap,
    caps: capabilities::Capabilities,
    pub(crate) module_loader: Box<dyn module_loader::ModuleLoader>,
    pub(crate) frontend: Option<Box<dyn xu_ir::Frontend>>,
    pub(crate) output: String,
    pub(crate) stmt_count: usize,
    pub(crate) structs: HashMap<String, StructDef>,
    pub(crate) struct_layouts: HashMap<String, std::rc::Rc<[String]>>,
    pub(crate) enums: HashMap<String, Vec<String>>,
    pub(crate) next_id: i64,
    pub(crate) main_invoked: bool,
    pub(crate) loaded_modules: HashMap<String, Value>,
    pub(crate) import_parse_cache: HashMap<String, modules::ImportParseCacheEntry>,
    pub(crate) import_stack: Vec<String>,
    pub(crate) entry_path: Option<String>,
    rng_state: u64,
    pub(crate) config: RuntimeConfig,
    pub(crate) locals: slot_allocator::LocalSlots,
    pub(crate) compiled_locals: HashMap<String, Vec<String>>,
    pub(crate) compiled_locals_idx: HashMap<String, HashMap<String, usize>>,
    pub(crate) current_func: Option<String>,
    pub(crate) current_param_bindings: Option<Vec<(String, usize)>>,
    method_cache: HashMap<(String, String), Value>,
    dict_cache: HashMap<(usize, u64), (u64, Text, Value)>,
    dict_cache_int: HashMap<(usize, i64), (u64, Value)>,
    pub(crate) dict_cache_last: Option<DictCacheLast>,
    pub(crate) dict_cache_int_last: Option<DictCacheIntLast>,
    pub(crate) dict_version_last: Option<(usize, u64)>,
    pub(crate) ic_slots: Vec<ICSlot>,
    pub(crate) ic_method_slots: Vec<MethodICSlot>,
    pub(crate) string_pool: HashMap<String, Rc<String>>,
    /// Pre-allocated string constant Values per Bytecode (keyed by Bytecode pointer)
    /// Each entry maps constant index to pre-allocated Value
    bytecode_string_cache: HashMap<usize, Vec<Option<Value>>>,
    pub(crate) stdlib_path: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) call_stack_depth: usize,
    predefined_constants: HashMap<String, String>,
    pub(crate) vm_stack_pool: Vec<Vec<Value>>,
    pub(crate) vm_iters_pool: Vec<Vec<ir::IterState>>,
    pub(crate) vm_handlers_pool: Vec<Vec<ir::Handler>>,
    builder_pool: Vec<String>,
    /// Cached Option::none value to avoid repeated allocations
    cached_option_none: Option<Value>,
    /// Temporary GC roots for values being evaluated (e.g., function arguments)
    pub(crate) gc_temp_roots: Vec<Value>,
    /// Active VM stacks that need GC protection (raw pointers for performance)
    pub(crate) active_vm_stacks: Vec<*const Vec<Value>>,
}

#[derive(Clone)]
pub(crate) struct DictCacheLast {
    pub(crate) id: usize,
    pub(crate) key_hash: u64,
    pub(crate) ver: u64,
    pub(crate) key: Text,
    pub(crate) value: Value,
}

#[derive(Clone)]
pub(crate) struct DictCacheIntLast {
    pub(crate) id: usize,
    pub(crate) key: i64,
    pub(crate) ver: u64,
    pub(crate) value: Value,
}

#[derive(Clone, Default)]
pub struct ICSlot {
    pub id: usize,
    pub key_hash: u64,
    pub key_id: usize, // Object ID of the key string for fast comparison
    pub key_short: [u8; 16], // Short key content for fast comparison
    pub key_len: u8,
    pub ver: u64,
    pub value: Value,
    pub option_some_cached: Value, // Cached Option::some(value) result
    pub struct_ty_hash: u64,
    pub field_offset: Option<usize>,
}

pub enum Flow {
    None,
    Return(Value),
    Break,
    Continue,
    Throw(Value),
}

impl Runtime {
    pub fn new() -> Self {
        Self::with_config(RuntimeConfig::default())
    }

    pub fn with_config(config: RuntimeConfig) -> Self {
        let env = Env::new();
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let mut rt = Self {
            env,
            env_pool: Vec::new(),
            heap: crate::gc::Heap::new(),
            caps: capabilities::Capabilities::default(),
            module_loader: Box::new(module_loader::StdModuleLoader),
            frontend: None,
            output: String::new(),
            stmt_count: 0,
            structs: fast_map_new(),
            struct_layouts: fast_map_new(),
            enums: fast_map_new(),
            next_id: 1,
            main_invoked: false,
            loaded_modules: fast_map_new(),
            import_parse_cache: fast_map_new(),
            import_stack: Vec::new(),
            entry_path: None,
            rng_state: seed,
            config,
            locals: slot_allocator::LocalSlots::new(),
            compiled_locals: fast_map_new(),
            compiled_locals_idx: fast_map_new(),
            current_func: None,
            current_param_bindings: None,
            method_cache: fast_map_new(),
            dict_cache: fast_map_new(),
            dict_cache_int: fast_map_new(),
            dict_cache_last: None,
            dict_cache_int_last: None,
            dict_version_last: None,
            ic_slots: Vec::new(),
            ic_method_slots: Vec::new(),
            string_pool: fast_map_new(),
            bytecode_string_cache: fast_map_new(),
            stdlib_path: None,
            args: Vec::new(),
            call_stack_depth: 0,
            predefined_constants: fast_map_new(),
            vm_stack_pool: Vec::new(),
            vm_iters_pool: Vec::new(),
            vm_handlers_pool: Vec::new(),
            builder_pool: Vec::new(),
            cached_option_none: None,
            gc_temp_roots: Vec::new(),
            active_vm_stacks: Vec::new(),
        };
        rt.install_builtins();
        rt
    }

    pub fn intern_string(&mut self, s: &str) -> Text {
        if s.len() <= 22 {
            // Text::INLINE_CAP
            return Text::from_str(s);
        }
        if let Some(rc) = self.string_pool.get(s) {
            return Text::Heap { data: rc.clone(), char_count: std::cell::Cell::new(u32::MAX) };
        }
        let rc = Rc::new(s.to_string());
        self.string_pool.insert(s.to_string(), rc.clone());
        Text::Heap { data: rc, char_count: std::cell::Cell::new(u32::MAX) }
    }

    /// Get or create a pre-allocated string Value for a bytecode constant.
    /// Uses bytecode pointer + constant index as cache key.
    #[inline]
    pub fn get_string_const(&mut self, bc_ptr: usize, idx: u32, s: &str) -> Value {
        // Fast path: check if we have a cached value
        if let Some(cache) = self.bytecode_string_cache.get(&bc_ptr) {
            if let Some(Some(val)) = cache.get(idx as usize) {
                return *val;
            }
        }
        // Slow path: create and cache the value
        let text = self.intern_string(s);
        let val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(text)));

        let cache = self.bytecode_string_cache.entry(bc_ptr).or_insert_with(Vec::new);
        let idx_usize = idx as usize;
        if cache.len() <= idx_usize {
            cache.resize(idx_usize + 1, None);
        }
        cache[idx_usize] = Some(val);
        val
    }

    pub(crate) fn option_none(&mut self) -> Value {
        if let Some(v) = self.cached_option_none {
            return v;
        }
        static OPTION: &str = "Option";
        static NONE: &str = "none";
        let v = Value::enum_obj(self.heap.alloc(crate::gc::ManagedObject::Enum(Box::new((
            crate::Text::from_str(OPTION),
            crate::Text::from_str(NONE),
            Box::new([]),
        )))));
        self.cached_option_none = Some(v);
        v
    }

    pub(crate) fn option_some(&mut self, v: Value) -> Value {
        // Use optimized OptionSome variant to avoid Box allocation
        Value::option_some(self.heap.alloc(crate::gc::ManagedObject::OptionSome(v)))
    }

    /// Get a String from the builder pool, or create a new one with the given capacity
    pub(crate) fn builder_pool_get(&mut self, cap: usize) -> String {
        if let Some(mut s) = self.builder_pool.pop() {
            s.clear();
            if s.capacity() < cap {
                s.reserve(cap - s.capacity());
            }
            s
        } else {
            String::with_capacity(cap)
        }
    }

    /// Return a String to the builder pool for reuse
    pub(crate) fn builder_pool_return(&mut self, s: String) {
        // Only keep strings with reasonable capacity to avoid memory bloat
        if s.capacity() <= 4096 && self.builder_pool.len() < 16 {
            self.builder_pool.push(s);
        }
    }

    pub fn define_global_constant(&mut self, name: &str, value: &str) {
        self.predefined_constants
            .insert(name.to_string(), value.to_string());
    }

    pub(crate) fn hash_bytes<S: BuildHasher>(build: &S, bytes: &[u8]) -> u64 {
        let mut h = build.build_hasher();
        h.write_u8(0);
        bytes.hash(&mut h);
        h.finish()
    }

    pub(crate) fn hash_dict_key_int<S: BuildHasher>(build: &S, i: i64) -> u64 {
        let mut h = build.build_hasher();
        h.write_u8(1);
        i.hash(&mut h);
        h.finish()
    }

    pub(crate) fn dict_get_by_str_with_hash(me: &Dict, key: &str, hash: u64) -> Option<Value> {
        me.map
            .raw_entry()
            .from_hash(hash, |k| match k {
                DictKey::Str { data, .. } => data.as_str() == key,
                _ => false,
            })
            .map(|(_, v)| v.clone())
    }

    pub(crate) fn enum_new_checked(
        &mut self,
        ty: &str,
        variant: &str,
        payload: Box<[Value]>,
    ) -> Result<Value, String> {
        if let Some(vars) = self.enums.get(ty) {
            if !vars.contains(&variant.to_string()) {
                return Err(self.error(xu_syntax::DiagnosticKind::UnknownEnumVariant(
                    ty.to_string(),
                    variant.to_string(),
                )));
            }
        }
        let id = self.heap.alloc(crate::gc::ManagedObject::Enum(Box::new((
            ty.to_string().into(),
            variant.to_string().into(),
            payload,
        ))));
        Ok(Value::enum_obj(id))
    }

    #[allow(dead_code)]
    pub(crate) fn enum_parts_cloned(
        &self,
        v: Value,
    ) -> Result<(crate::Text, crate::Text, Box<[Value]>), String> {
        if v.get_tag() != crate::value::TAG_ENUM {
            return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-enum object".into())));
        }
        match self.heap.get(v.as_obj_id()) {
            crate::gc::ManagedObject::Enum(e) => {
                let (ty, variant, payload) = e.as_ref();
                Ok((
                    ty.clone(),
                    variant.clone(),
                    payload
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ))
            }
            _ => Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-enum object".into()))),
        }
    }

    pub fn set_clock(&mut self, clock: Box<dyn capabilities::Clock>) {
        self.caps.clock = clock;
    }

    pub fn set_file_system(&mut self, fs: Box<dyn capabilities::FileSystem>) {
        self.caps.fs = fs;
    }

    pub fn set_rng_algorithm(&mut self, rng: Box<dyn capabilities::RngAlgorithm>) {
        self.caps.rng = rng;
    }

    pub fn set_module_loader(&mut self, loader: Box<dyn module_loader::ModuleLoader>) {
        self.module_loader = loader;
    }

    pub fn set_frontend(&mut self, frontend: Box<dyn xu_ir::Frontend>) {
        self.frontend = Some(frontend);
    }

    pub fn clear_allowed_roots(&mut self) {
        self.caps.allowed_roots.clear();
    }

    pub fn add_allowed_root(&mut self, path: &str) -> Result<(), String> {
        let canonical = self
            .caps
            .fs
            .canonicalize(path)
            .map_err(|e| format!("Failed to set allowed root directory: {e}"))?;
        self.caps.allowed_roots.push(canonical);
        Ok(())
    }

    pub fn path_allowed(&self, path: &str) -> bool {
        if self.caps.allowed_roots.is_empty() {
            return true;
        }
        let p = std::path::PathBuf::from(path);
        for root in &self.caps.allowed_roots {
            let r = std::path::Path::new(root);
            if p.starts_with(r) {
                return true;
            }
        }
        false
    }

    pub(crate) fn canonicalize_import_checked(&self, path: &str) -> Result<String, String> {
        let p = std::path::Path::new(path);
        let p = if p.is_relative() {
            p.to_path_buf()
        } else {
            p.to_path_buf()
        };

        if !p.exists() {
            return Err(format!("File not found: {path}"));
        }

        let abs = p
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize path: {e}"))?;
        let abs = abs.to_string_lossy().to_string();
        if !self.path_allowed(&abs) {
            return Err(self.error(xu_syntax::DiagnosticKind::PathNotAllowed));
        }
        Ok(abs)
    }

    pub fn set_rng_seed(&mut self, seed: u64) {
        self.rng_state = seed;
    }

    pub fn set_stdlib_path(&mut self, path: String) {
        self.stdlib_path = Some(path);
    }

    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    pub fn stdlib_path(&self) -> Option<&str> {
        self.stdlib_path.as_deref()
    }

    pub fn set_strict_vars(&mut self, enabled: bool) {
        self.config.strict_vars = enabled;
    }

    pub fn set_entry_path(&mut self, path: &str) -> Result<(), String> {
        let canonical =
            std::fs::canonicalize(path).map_err(|e| format!("Failed to set entry path: {e}"))?;
        self.entry_path = Some(canonical.to_string_lossy().to_string());
        Ok(())
    }

    pub fn has_builtin(&self, name: &str) -> bool {
        self.env.get(name).is_some()
    }

    pub(crate) fn push_locals(&mut self) {
        self.locals.push();
    }

    pub(crate) fn pop_locals(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn get_local(&self, name: &str) -> Option<Value> {
        self.locals.get(name)
    }

    pub(crate) fn get_local_by_index(&self, idx: usize) -> Option<Value> {
        self.locals.get_by_index(idx)
    }

    pub(crate) fn get_local_by_depth_index(&self, depth_from_top: usize, idx: usize) -> Option<Value> {
        self.locals.get_by_depth_index(depth_from_top, idx)
    }

    pub(crate) fn set_local(&mut self, name: &str, value: Value) -> bool {
        let value_for_env = value.clone();
        if self.locals.set(name, value) {
            let _ = value_for_env; // skip env updates for local fast path
            return true;
        }
        false
    }

    pub(crate) fn get_local_index(&self, name: &str) -> Option<usize> {
        self.locals.get_index(name)
    }

    pub(crate) fn set_local_by_index(&mut self, idx: usize, value: Value) -> bool {
        if self.locals.set_by_index(idx, value) {
            return true;
        }
        false
    }

    pub(crate) fn define_local(&mut self, name: String, value: Value) {
        let _ = self.locals.define(name, value);
    }

    pub(crate) fn define_local_with_mutability(&mut self, name: String, value: Value, immutable: bool) {
        let _ = self.locals.define_with_mutability(name, value, immutable);
    }

    pub(crate) fn is_local_immutable(&self, name: &str) -> bool {
        self.locals.is_immutable(name)
    }

    pub(crate) fn get_constant<'a>(
        &self,
        idx: u32,
        constants: &'a [xu_ir::Constant],
    ) -> &'a xu_ir::Constant {
        &constants[idx as usize]
    }

    pub(crate) fn get_const_str<'a>(&self, idx: u32, constants: &'a [xu_ir::Constant]) -> &'a str {
        match &constants[idx as usize] {
            xu_ir::Constant::Str(s) => s,
            _ => panic!("Expected string constant"),
        }
    }

    pub(crate) fn get_const_names<'a>(
        &self,
        idx: u32,
        constants: &'a [xu_ir::Constant],
    ) -> &'a [String] {
        match &constants[idx as usize] {
            xu_ir::Constant::Names(names) => names,
            _ => panic!("Expected names constant"),
        }
    }

    pub fn exec_module(&mut self, module: &Module) -> Result<ExecResult, String> {
        self.reset_for_entry_execution();
        self.compiled_locals = Self::collect_func_locals(module);
        self.compiled_locals_idx = Self::index_func_locals(&self.compiled_locals);
        Self::precompile_module(module)?;
        let flow = self.exec_stmts(&module.stmts);
        match flow {
            Flow::None => {
                self.invoke_main_if_present()?;
                Ok(ExecResult {
                    value: None,
                    output: std::mem::take(&mut self.output),
                })
            }
            Flow::Return(v) => Ok(ExecResult {
                value: Some(v),
                output: std::mem::take(&mut self.output),
            }),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => {
                Err(self.error(xu_syntax::DiagnosticKind::TopLevelBreakContinue))
            }
        }
    }

    pub fn exec_executable(&mut self, executable: &Executable) -> Result<ExecResult, String> {
        match executable {
            Executable::Ast(module) => self.exec_module(module),
            Executable::Bytecode(program) => self.exec_program(program),
        }
    }

    pub fn exec_program(&mut self, program: &xu_ir::Program) -> Result<ExecResult, String> {
        self.reset_for_entry_execution();
        self.compiled_locals = Self::collect_func_locals(&program.module);
        self.compiled_locals_idx = Self::index_func_locals(&self.compiled_locals);
        for s in &program.module.stmts {
            match s {
                Stmt::StructDef(def) => {
                    self.structs.insert(def.name.clone(), (**def).clone());
                }
                Stmt::EnumDef(def) => {
                    self.enums.insert(def.name.clone(), def.variants.to_vec());
                }
                _ => {}
            }
        }
        Self::precompile_module(&program.module)?;
        let flow = if let Some(bc) = program.bytecode.as_ref() {
            ir::run_bytecode(self, bc)?
        } else {
            self.exec_stmts(&program.module.stmts)
        };
        match flow {
            Flow::None => {
                self.invoke_main_if_present()?;
                Ok(ExecResult {
                    value: None,
                    output: std::mem::take(&mut self.output),
                })
            }
            Flow::Return(v) => Ok(ExecResult {
                value: Some(v),
                output: std::mem::take(&mut self.output),
            }),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => {
                Err(self.error(xu_syntax::DiagnosticKind::TopLevelBreakContinue))
            }
        }
    }

    pub(crate) fn reset_for_entry_execution(&mut self) {
        self.output.clear();
        self.main_invoked = false;
        self.import_stack.clear();
        self.loaded_modules.clear();
        self.structs.clear();
        self.enums.clear();
        self.next_id = 1;
        self.locals.clear();

        self.env = Env::new();
        self.heap = crate::gc::Heap::new();
        self.install_builtins();
        for (k, v) in &self.predefined_constants {
            let s = self
                .heap
                .alloc(crate::gc::ManagedObject::Str(v.to_string().into()));
            self.env.define(k.clone(), Value::str(s));
        }
        self.method_cache.clear();
        self.dict_cache.clear();
        self.dict_cache_int.clear();
        self.dict_cache_last = None;
        self.dict_cache_int_last = None;
        self.dict_version_last = None;
        self.ic_slots.clear();
        self.ic_method_slots.clear();
        self.current_param_bindings = None;
        self.call_stack_depth = 0;
    }

    fn invoke_main_if_present(&mut self) -> Result<(), String> {
        if self.main_invoked {
            return Ok(());
        }
        let Some(v) = self.env.get("main") else {
            return Ok(());
        };
        if v.get_tag() == crate::value::TAG_FUNC {
            self.main_invoked = true;
            let _ = self.call_function(v, &[])?;
            Ok(())
        } else {
            Ok(())
        }
    }

    pub(crate) fn install_builtins(&mut self) {
        let mut registry = builtins_registry::BuiltinRegistry::new();
        builtins_registry::BuiltinProvider::install(
            &builtins_registry::StdBuiltinProvider,
            &mut registry,
        );
        registry.install_into(&mut self.env, &mut self.heap);
    }

    pub(crate) fn clock_unix_secs(&self) -> i64 {
        self.caps.clock.unix_secs()
    }

    pub(crate) fn clock_unix_millis(&self) -> i64 {
        self.caps.clock.unix_millis()
    }

    pub(crate) fn clock_mono_micros(&self) -> i64 {
        self.caps.clock.mono_micros()
    }

    pub(crate) fn clock_mono_nanos(&self) -> i64 {
        self.caps.clock.mono_nanos()
    }

    pub(crate) fn fs_metadata(&self, path: &str) -> Result<(), String> {
        self.caps
            .fs
            .metadata(path)
            .map_err(|e| format!("Open failed: {e}"))
    }

    pub(crate) fn fs_read_to_string(&self, path: &str) -> Result<String, String> {
        self.caps
            .fs
            .read_to_string(path)
            .map_err(|e| format!("Read failed: {e}"))
    }

    pub(crate) fn fs_read_to_string_import(&self, path: &str) -> Result<String, String> {
        self.caps
            .fs
            .read_to_string(path)
            .map_err(|e| format!("Import failed: {e}"))
    }

    pub(crate) fn fs_stat(&self, path: &str) -> Result<capabilities::FileStat, String> {
        self.caps
            .fs
            .stat(path)
            .map_err(|e| format!("Import failed: {e}"))
    }

    pub(crate) fn rng_next_u64(&mut self) -> u64 {
        self.caps.rng.next_u64(&mut self.rng_state)
    }

    pub(crate) fn error(&self, kind: xu_syntax::DiagnosticKind) -> String {
        xu_syntax::DiagnosticsFormatter::format(&kind)
    }

    pub(crate) fn collect_func_locals(module: &Module) -> HashMap<String, Vec<String>> {
        use std::collections::HashSet;
        fn push_unique(ordered: &mut Vec<String>, seen: &mut HashSet<String>, name: &str) {
            if seen.insert(name.to_string()) {
                ordered.push(name.to_string());
            }
        }
        fn walk_stmts(ordered: &mut Vec<String>, seen: &mut HashSet<String>, stmts: &[Stmt]) {
            for s in stmts {
                match s {
                    Stmt::Assign(a) => {
                        if let Expr::Ident(n, _) = &a.target {
                            push_unique(ordered, seen, n);
                        }
                    }
                    Stmt::ForEach(fe) => {
                        push_unique(ordered, seen, &fe.var);
                        walk_stmts(ordered, seen, &fe.body);
                    }
                    Stmt::If(i) => {
                        for (cond, body) in &i.branches {
                            let _ = cond;
                            walk_stmts(ordered, seen, body);
                        }
                        if let Some(b) = &i.else_branch {
                            walk_stmts(ordered, seen, b);
                        }
                    }
                    Stmt::While(w) => walk_stmts(ordered, seen, &w.body),
                    Stmt::Block(stmts) => walk_stmts(ordered, seen, stmts),
                    Stmt::FuncDef(_) => {}
                    _ => {}
                }
            }
        }
        let mut out: HashMap<String, Vec<String>> = fast_map_new();
        for s in &module.stmts {
            if let Stmt::FuncDef(fd) = s {
                let mut ordered: Vec<String> = Vec::new();
                let mut names = std::collections::HashSet::new();
                for p in &fd.params {
                    push_unique(&mut ordered, &mut names, &p.name);
                }
                walk_stmts(&mut ordered, &mut names, &fd.body);
                out.insert(fd.name.clone(), ordered);
            }
        }
        out
    }

    pub(crate) fn index_func_locals(
        map: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, HashMap<String, usize>> {
        let mut out = fast_map_new();
        for (fname, names) in map.iter() {
            let mut idxmap = fast_map_new();
            for (i, n) in names.iter().enumerate() {
                idxmap.insert(n.clone(), i);
            }
            out.insert(fname.clone(), idxmap);
        }
        out
    }

    pub(crate) fn call_method_with_ic_raw(
        &mut self,
        recv: Value,
        method: &str,
        method_hash: u64,
        args: &[Value],
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = recv.get_tag();

        // IC check
        if let Some(idx) = slot_idx {
            if idx < self.ic_method_slots.len() {
                let slot = &self.ic_method_slots[idx];
                if slot.tag == tag && slot.method_hash == method_hash {
                    if tag == crate::value::TAG_STRUCT {
                        let id = recv.as_obj_id();
                        if let crate::gc::ManagedObject::Struct(s) = self.heap.get(id) {
                            if slot.struct_ty_hash == s.ty_hash {
                                if let Some(f) = slot.cached_bytecode.as_ref() {
                                    if args.is_empty() {
                                        return self.call_bytecode_function(f.clone(), &[recv]);
                                    }
                                    if args.len() == 1 {
                                        let all = [recv, args[0]];
                                        return self.call_bytecode_function(f.clone(), &all);
                                    }
                                }
                                let mut all_args: SmallVec<[Value; 4]> =
                                    SmallVec::with_capacity(args.len() + 1);
                                all_args.push(recv);
                                all_args.extend(args.iter().cloned());
                                if let Some(f) = slot.cached_user.as_ref() {
                                    return self.call_user_function(f.clone(), &all_args);
                                }
                                return self.call_function(slot.cached_func, &all_args);
                            }
                        }
                    } else if tag == crate::value::TAG_ENUM {
                        let id = recv.as_obj_id();
                        if let crate::gc::ManagedObject::Enum(e) =
                            self.heap.get(id)
                        {
                            let (ty, _variant, _payload) = e.as_ref();
                            let ty_hash = xu_ir::stable_hash64(ty.as_str());
                            if slot.struct_ty_hash == ty_hash {
                                if let Some(f) = slot.cached_bytecode.as_ref() {
                                    if args.is_empty() {
                                        return self.call_bytecode_function(f.clone(), &[recv]);
                                    }
                                    if args.len() == 1 {
                                        let all = [recv, args[0]];
                                        return self.call_bytecode_function(f.clone(), &all);
                                    }
                                }
                                let mut all_args: SmallVec<[Value; 4]> =
                                    SmallVec::with_capacity(args.len() + 1);
                                all_args.push(recv);
                                all_args.extend(args.iter().cloned());
                                if let Some(f) = slot.cached_user.as_ref() {
                                    return self.call_user_function(f.clone(), &all_args);
                                }
                                return self.call_function(slot.cached_func, &all_args);
                            }
                        }
                    } else if slot.kind != MethodKind::Unknown {
                        return methods::dispatch_builtin_method(
                            self, recv, slot.kind, args, method,
                        );
                    }
                }
            }
        }

        if tag == crate::value::TAG_MODULE {
            let id = recv.as_obj_id();
            let callee = if let crate::gc::ManagedObject::Module(m) = self.heap.get(id) {
                m.exports.map.get(method).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(method.to_string()))
                })?
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-module object".into())));
            };
            if callee.get_tag() != crate::value::TAG_FUNC {
                return Err(self.error(xu_syntax::DiagnosticKind::NotCallable(method.to_string())));
            }
            // Update IC
            if let Some(idx) = slot_idx {
                while self.ic_method_slots.len() <= idx {
                    self.ic_method_slots.push(MethodICSlot::default());
                }
                self.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: 0,
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::gc::ManagedObject::Function(
                        crate::value::Function::User(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::gc::ManagedObject::Function(
                        crate::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }
            self.call_function(callee, &args)
        } else if tag == crate::value::TAG_STRUCT {
            let id = recv.as_obj_id();
            let callee = match if let crate::gc::ManagedObject::Struct(s) = self.heap.get(id) {
                let ty = s.ty.as_str();
                let hash = {
                    let mut h = self.method_cache.hasher().build_hasher();
                    ty.hash(&mut h);
                    method.hash(&mut h);
                    h.finish()
                };
                match self
                    .method_cache
                    .raw_entry_mut()
                    .from_hash(hash, |(t, m)| t == ty && m == method)
                {
                    RawEntryMut::Occupied(o) => Ok(o.get().clone()),
                    RawEntryMut::Vacant(vac) => {
                        let name = format!("__method__{}__{}", ty, method);
                        if let Some(v) = self.env.get_cached(&name) {
                            vac.insert((s.ty.clone(), method.to_string()), v.clone());
                            Ok(v)
                        } else {
                            Err(xu_syntax::DiagnosticKind::UnknownMember(method.to_string()))
                        }
                    }
                }
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-struct object".into())));
            } {
                Ok(v) => v,
                Err(kind) => return Err(self.error(kind)),
            };

            // Update IC
            if let Some(idx) = slot_idx {
                while self.ic_method_slots.len() <= idx {
                    self.ic_method_slots.push(MethodICSlot::default());
                }
                self.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: if let crate::gc::ManagedObject::Struct(s) = self.heap.get(id) {
                        s.ty_hash
                    } else {
                        0
                    },
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::gc::ManagedObject::Function(
                        crate::value::Function::User(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::gc::ManagedObject::Function(
                        crate::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }

            let mut all_args: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len() + 1);
            all_args.push(recv);
            all_args.extend(args.iter().cloned());
            self.call_function(callee, &all_args)
        } else if tag == crate::value::TAG_ENUM {
            let id = recv.as_obj_id();
            let (callee, ty_hash) =
                match if let crate::gc::ManagedObject::Enum(e) =
                    self.heap.get(id)
                {
                    let (ty, _variant, _payload) = e.as_ref();
                    let ty_str = ty.as_str();
                    let hash = {
                        let mut h = self.method_cache.hasher().build_hasher();
                        ty_str.hash(&mut h);
                        method.hash(&mut h);
                        h.finish()
                    };
                    match self
                        .method_cache
                        .raw_entry_mut()
                        .from_hash(hash, |(t, m)| t == ty_str && m == method)
                    {
                        RawEntryMut::Occupied(o) => Ok((o.get().clone(), xu_ir::stable_hash64(ty_str))),
                        RawEntryMut::Vacant(vac) => {
                            let name = format!("__method__{}__{}", ty_str, method);
                            if let Some(v) = self.env.get_cached(&name) {
                                vac.insert((ty.to_string(), method.to_string()), v.clone());
                                Ok((v, xu_ir::stable_hash64(ty_str)))
                            } else {
                                Err(())
                            }
                        }
                    }
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-enum object".into())));
                } {
                    Ok(v) => v,
                    Err(()) => {
                        let kind = MethodKind::from_str(method);
                        if kind == MethodKind::Unknown {
                            return Err(self.error(xu_syntax::DiagnosticKind::UnknownMember(
                                method.to_string(),
                            )));
                        }
                        if let Some(idx) = slot_idx {
                            while self.ic_method_slots.len() <= idx {
                                self.ic_method_slots.push(MethodICSlot::default());
                            }
                            self.ic_method_slots[idx] = MethodICSlot {
                                tag,
                                method_hash,
                                struct_ty_hash: 0,
                                kind,
                                cached_func: Value::VOID,
                                cached_user: None,
                                cached_bytecode: None,
                            };
                        }
                        return methods::dispatch_builtin_method(self, recv, kind, args, method);
                    }
                };

            if let Some(idx) = slot_idx {
                while self.ic_method_slots.len() <= idx {
                    self.ic_method_slots.push(MethodICSlot::default());
                }
                self.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: ty_hash,
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::gc::ManagedObject::Function(crate::value::Function::User(
                        f,
                    )) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::gc::ManagedObject::Function(
                        crate::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }

            if callee.get_tag() != crate::value::TAG_FUNC {
                return Err(self.error(xu_syntax::DiagnosticKind::NotCallable(method.to_string())));
            }
            let mut all_args: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len() + 1);
            all_args.push(recv);
            all_args.extend(args.iter().cloned());
            self.call_function(callee, &all_args)
        } else {
            let kind = MethodKind::from_str(method);
            if kind == MethodKind::Unknown {
                return Err(self.error(xu_syntax::DiagnosticKind::UnsupportedReceiver(
                    recv.type_name().to_string(),
                )));
            }

            // Update IC
            if let Some(idx) = slot_idx {
                while self.ic_method_slots.len() <= idx {
                    self.ic_method_slots.push(MethodICSlot::default());
                }
                self.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: 0,
                    kind,
                    cached_func: Value::VOID,
                    cached_user: None,
                    cached_bytecode: None,
                };
            }

            methods::dispatch_builtin_method(self, recv, kind, args, method)
        }
    }

    pub(crate) fn eval_binary(&mut self, op: BinaryOp, a: Value, b: Value) -> Result<Value, String> {
        let debug_err = |e: String, a: &Value, b: &Value, op: BinaryOp, heap: &crate::gc::Heap| {
            let sa = value_to_string(a, heap);
            let sb = value_to_string(b, heap);
            println!("BinaryOp Error: {} {:?} {} -> {}", sa, op, sb, e);
            Err(e)
        };

        match op {
            BinaryOp::Eq => Ok(Value::from_bool(self.values_equal(&a, &b))),
            BinaryOp::Ne => Ok(Value::from_bool(!self.values_equal(&a, &b))),
            BinaryOp::Add => {
                let at = a.get_tag();
                let bt = b.get_tag();
                if at == crate::value::TAG_STR && bt == crate::value::TAG_STR {
                    // Fast path: both are strings - use Text::concat2
                    let ta = if let crate::gc::ManagedObject::Str(s) = self.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        Text::new()
                    };
                    let tb = if let crate::gc::ManagedObject::Str(s) = self.heap.get(b.as_obj_id()) {
                        s.clone()
                    } else {
                        Text::new()
                    };
                    let result = Text::concat2(&ta, &tb);
                    Ok(Value::str(
                        self.heap.alloc(crate::gc::ManagedObject::Str(result)),
                    ))
                } else if at == crate::value::TAG_STR || bt == crate::value::TAG_STR {
                    let sa = value_to_string(&a, &self.heap);
                    let sb = value_to_string(&b, &self.heap);
                    // Pre-allocate capacity to avoid intermediate allocations
                    let mut result = String::with_capacity(sa.len() + sb.len());
                    result.push_str(&sa);
                    result.push_str(&sb);
                    Ok(Value::str(
                        self.heap
                            .alloc(crate::gc::ManagedObject::Str(result.into())),
                    ))
                } else {
                    a.bin_op(op, b)
                        .or_else(|e| debug_err(e, &a, &b, op, &self.heap))
                }
            }
            BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                if a.get_tag() == crate::value::TAG_STR && b.get_tag() == crate::value::TAG_STR {
                    let sa = if let crate::gc::ManagedObject::Str(s) = self.heap.get(a.as_obj_id())
                    {
                        s.as_str().to_string()
                    } else {
                        String::new()
                    };
                    let sb = if let crate::gc::ManagedObject::Str(s) = self.heap.get(b.as_obj_id())
                    {
                        s.as_str().to_string()
                    } else {
                        String::new()
                    };
                    let res = match op {
                        BinaryOp::Gt => sa > sb,
                        BinaryOp::Lt => sa < sb,
                        BinaryOp::Ge => sa >= sb,
                        BinaryOp::Le => sa <= sb,
                        _ => unreachable!(),
                    };
                    Ok(Value::from_bool(res))
                } else {
                    a.bin_op(op, b)
                        .or_else(|e| debug_err(e, &a, &b, op, &self.heap))
                }
            }
            _ => a
                .bin_op(op, b)
                .or_else(|e| debug_err(e, &a, &b, op, &self.heap)),
        }
    }

    #[inline(always)]
    pub(crate) fn values_equal(&self, a: &Value, b: &Value) -> bool {
        if a == b {
            return true;
        }
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        self.values_equal_inner(a, b, &mut seen)
    }

    fn values_equal_inner(&self, a: &Value, b: &Value, seen: &mut HashSet<(usize, usize)>) -> bool {
        if a == b {
            return true;
        }

        if a.is_int() && b.is_f64() {
            return (a.as_i64() as f64) == b.as_f64();
        }
        if a.is_f64() && b.is_int() {
            return a.as_f64() == (b.as_i64() as f64);
        }

        let at = a.get_tag();
        let bt = b.get_tag();
        if at != bt {
            return false;
        }

        if a.is_obj() {
            let aid = a.as_obj_id();
            let bid = b.as_obj_id();
            let key = (aid.0, bid.0);
            if !seen.insert(key) {
                return true;
            }

            match (self.heap.get(aid), self.heap.get(bid)) {
                (crate::gc::ManagedObject::Str(x), crate::gc::ManagedObject::Str(y)) => x == y,
                (
                    crate::gc::ManagedObject::List(a_list),
                    crate::gc::ManagedObject::List(b_list),
                ) => {
                    if a_list.len() != b_list.len() {
                        return false;
                    }
                    for (x, y) in a_list.iter().zip(b_list.iter()) {
                        if !self.values_equal_inner(x, y, seen) {
                            return false;
                        }
                    }
                    true
                }
                (
                    crate::gc::ManagedObject::Dict(a_dict),
                    crate::gc::ManagedObject::Dict(b_dict),
                ) => {
                    if a_dict.map.len() != b_dict.map.len() {
                        return false;
                    }
                    for (k, av) in a_dict.map.iter() {
                        let Some(bv) = b_dict.map.get(k) else {
                            return false;
                        };
                        if !self.values_equal_inner(av, bv, seen) {
                            return false;
                        }
                    }
                    true
                }
                (
                    crate::gc::ManagedObject::Range(a1, a2, ai),
                    crate::gc::ManagedObject::Range(b1, b2, bi),
                ) => a1 == b1 && a2 == b2 && ai == bi,
                (
                    crate::gc::ManagedObject::Enum(ea),
                    crate::gc::ManagedObject::Enum(eb),
                ) => {
                    let (ta, va, pa) = ea.as_ref();
                    let (tb, vb, pb) = eb.as_ref();
                    if ta != tb || va != vb || pa.len() != pb.len() {
                        return false;
                    }
                    for (x, y) in pa.iter().zip(pb.iter()) {
                        if !self.values_equal_inner(x, y, seen) {
                            return false;
                        }
                    }
                    true
                }
                (crate::gc::ManagedObject::Struct(as_), crate::gc::ManagedObject::Struct(bs)) => {
                    if as_.ty != bs.ty || as_.fields.len() != bs.fields.len() {
                        return false;
                    }
                    for i in 0..as_.fields.len() {
                        if !self.values_equal_inner(&as_.fields[i], &bs.fields[i], seen) {
                            return false;
                        }
                    }
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    pub(crate) fn format_throw(&self, v: &Value) -> String {
        if v.get_tag() == crate::value::TAG_STR {
            if let crate::gc::ManagedObject::Str(s) = self.heap.get(v.as_obj_id()) {
                return s.to_string();
            }
        }
        format!("{v:?}")
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    pub fn write_output(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    pub(crate) fn precompile_module(module: &Module) -> Result<(), String> {
        Self::precompile_stmts(&module.stmts)
    }

    fn precompile_stmts(stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            match s {
                Stmt::StructDef(_) => {}
                Stmt::EnumDef(_) => {}
                Stmt::FuncDef(def) => {
                    Self::precompile_stmts(&def.body)?;
                    for p in &def.params {
                        if let Some(d) = &p.default {
                            Self::precompile_expr(d)?;
                        }
                    }
                }
                Stmt::DoesBlock(def) => {
                    for def in def.funcs.iter() {
                        Self::precompile_stmts(&def.body)?;
                        for p in &def.params {
                            if let Some(d) = &p.default {
                                Self::precompile_expr(d)?;
                            }
                        }
                    }
                }
                Stmt::Use(_) => {}
                Stmt::If(s) => {
                    for (cond, body) in &s.branches {
                        Self::precompile_expr(cond)?;
                        Self::precompile_stmts(body)?;
                    }
                    if let Some(body) = &s.else_branch {
                        Self::precompile_stmts(body)?;
                    }
                }
                Stmt::While(s) => {
                    Self::precompile_expr(&s.cond)?;
                    Self::precompile_stmts(&s.body)?;
                }
                Stmt::ForEach(s) => {
                    Self::precompile_expr(&s.iter)?;
                    Self::precompile_stmts(&s.body)?;
                }
                Stmt::Match(s) => {
                    Self::precompile_expr(&s.expr)?;
                    for (_, body) in s.arms.iter() {
                        Self::precompile_stmts(body)?;
                    }
                    if let Some(body) = &s.else_branch {
                        Self::precompile_stmts(body)?;
                    }
                }
                Stmt::Return(e) => {
                    if let Some(e) = e {
                        Self::precompile_expr(e)?;
                    }
                }
                Stmt::Assign(s) => {
                    Self::precompile_expr(&s.target)?;
                    Self::precompile_expr(&s.value)?;
                }
                Stmt::Expr(e) => Self::precompile_expr(e)?,
                Stmt::Block(stmts) => Self::precompile_stmts(stmts)?,
                Stmt::Break | Stmt::Continue => {}
                Stmt::Error(_) => {}
            }
        }
        Ok(())
    }

    fn precompile_expr(expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Str(_) => Ok(()),
            Expr::EnumCtor { .. } => Ok(()),
            Expr::InterpolatedString(parts) => {
                for e in parts {
                    // Expr::Null is gone, no need to check for it specifically?
                    // But if interpolation has Empty/Unit expression?
                    // Expr can't be Unit directly, only via Block/Tuple.
                    // Let's just recurse.
                    Self::precompile_expr(e)?;
                }
                Ok(())
            }
            Expr::List(items) => {
                for e in items {
                    Self::precompile_expr(e)?;
                }
                Ok(())
            }
            Expr::Range(r) => {
                Self::precompile_expr(&r.start)?;
                Self::precompile_expr(&r.end)
            }
            Expr::IfExpr(e) => {
                Self::precompile_expr(&e.cond)?;
                Self::precompile_expr(&e.then_expr)?;
                Self::precompile_expr(&e.else_expr)
            }
            Expr::Dict(entries) => {
                for (_, v) in entries {
                    Self::precompile_expr(v)?;
                }
                Ok(())
            }
            Expr::StructInit(s) => {
                for item in s.items.iter() {
                    match item {
                        xu_ir::StructInitItem::Spread(e) => Self::precompile_expr(e)?,
                        xu_ir::StructInitItem::Field(_, v) => Self::precompile_expr(v)?,
                    }
                }
                Ok(())
            }
            Expr::Member(m) => Self::precompile_expr(&m.object),
            Expr::Index(m) => {
                Self::precompile_expr(&m.object)?;
                Self::precompile_expr(&m.index)
            }
            Expr::Call(c) => {
                Self::precompile_expr(&c.callee)?;
                for a in c.args.iter() {
                    Self::precompile_expr(a)?;
                }
                Ok(())
            }
            Expr::MethodCall(m) => {
                Self::precompile_expr(&m.receiver)?;
                for a in m.args.iter() {
                    Self::precompile_expr(a)?;
                }
                Ok(())
            }
            Expr::Unary { expr, .. } => Self::precompile_expr(expr),
            Expr::Binary { left, right, .. } => {
                Self::precompile_expr(left)?;
                Self::precompile_expr(right)
            }
            Expr::Group(e) => Self::precompile_expr(e),
            Expr::Ident(..) | Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) => Ok(()),
            _ => Ok(()),
        }
    }
    pub fn gc(&mut self, extra_roots: &[Value]) {
        // Clear caches that are safe to clear (don't affect correctness)
        self.method_cache.clear();
        self.dict_cache.clear();
        self.dict_cache_int.clear();
        self.dict_cache_last = None;
        self.dict_cache_int_last = None;
        self.dict_version_last = None;
        self.ic_slots.clear();
        self.ic_method_slots.clear();

        // Create roots vector
        let mut roots: Vec<Value> = Vec::new();
        roots.extend_from_slice(extra_roots);

        // Add temporary GC roots (e.g., function arguments being evaluated)
        roots.extend_from_slice(&self.gc_temp_roots);

        // Add values from active VM stacks
        for stack_ptr in &self.active_vm_stacks {
            // SAFETY: The stack pointer is valid as long as the VM frame is active
            let stack = unsafe { &**stack_ptr };
            for val in stack {
                roots.push(*val);
            }
        }

        // Add stack values as roots
        for val in &self.env.stack {
            roots.push(val.clone());
        }

        // Add all frame values as roots (not just global frame)
        for frame in &self.env.frames {
            let scope = frame.scope.borrow();
            for val in &scope.values {
                roots.push(val.clone());
            }
        }

        // Add local slot values as roots
        for frame_values in &self.locals.values {
            for val in frame_values {
                roots.push(val.clone());
            }
        }

        // Add bytecode string cache values as roots
        for cache in self.bytecode_string_cache.values() {
            for val in cache.iter().flatten() {
                roots.push(*val);
            }
        }

        // Mark all reachable objects
        self.heap.mark_all(&roots, &[&self.env], &[&self.locals]);

        // Sweep phase
        self.heap.sweep();
    }

    pub(crate) fn maybe_gc(&mut self) {
        if self.heap.should_gc() {
            self.gc(&[]);
        }
    }

    pub(crate) fn maybe_gc_with_roots(&mut self, roots: &[Value]) {
        if self.heap.should_gc() {
            self.gc(roots);
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

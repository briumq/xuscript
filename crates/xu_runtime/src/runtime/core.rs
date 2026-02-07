use std::hash::{BuildHasher, Hash, Hasher};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use indexmap::map::RawEntryApiV1;
use xu_ir::{Executable, Expr, Module, Stmt};

use crate::core::Value;
use crate::core::Env;
use crate::core::value::{Dict, DictKey, FastHashMap, fast_map_new};
use crate::util as capabilities;
use crate::modules;
use crate::core::slot_allocator;
use crate::vm as ir;
use crate::builtins_registry;

type HashMap<K, V> = FastHashMap<K, V>;

// Re-exports
pub use crate::core::text::Text;

// Import types from sibling modules
use super::config::{ExecResult, Flow, RuntimeConfig};
use super::type_system::TypeSystem;
use super::cache_manager::CacheManager;
use super::object_pools::ObjectPools;

/// Runtime 结构体 - xuscript 的核心执行引擎
/// 字段已按功能分组为子结构体以提高可维护性：
/// - `types`: 类型系统（结构体、枚举、静态字段）
/// - `caches`: 缓存管理（方法缓存、IC 槽、字符串池）
/// - `pools`: 对象池（环境池、VM 栈池等）
pub struct Runtime {
    // ==================== 核心执行状态 ====================
    pub(crate) env: Env,
    pub(crate) heap: crate::core::heap::Heap,
    #[cfg(feature = "generational-gc")]
    pub(crate) gen_heap: crate::core::generational_heap::GenerationalHeap,
    caps: capabilities::Capabilities,
    pub(crate) output: String,
    pub(crate) main_invoked: bool,
    pub(crate) call_stack_depth: usize,
    rng_state: u64,
    pub(crate) config: RuntimeConfig,

    // ==================== 类型系统 ====================
    /// 类型系统管理器（结构体、枚举、静态字段）
    pub(crate) types: TypeSystem,

    // ==================== 缓存管理 ====================
    /// 缓存管理器（方法缓存、IC 槽、字符串池）
    pub(crate) caches: CacheManager,

    // ==================== 对象池 ====================
    /// 对象池管理器（环境池、VM 栈池等）
    pub(crate) pools: ObjectPools,

    // ==================== 模块系统 ====================
    pub(crate) module_loader: Box<dyn modules::ModuleLoader>,
    pub(crate) frontend: Option<Box<dyn xu_ir::Frontend>>,
    pub(crate) loaded_modules: HashMap<String, Value>,
    pub(crate) import_parse_cache: HashMap<String, modules::ImportParseCacheEntry>,
    pub(crate) import_stack: Vec<String>,
    pub(crate) entry_path: Option<String>,

    // ==================== 局部变量管理 ====================
    pub(crate) locals: slot_allocator::LocalSlots,
    pub(crate) compiled_locals: HashMap<String, Vec<String>>,
    pub(crate) compiled_locals_idx: HashMap<String, HashMap<String, usize>>,
    pub(crate) current_func: Option<String>,
    /// 当前函数进入时的局部变量帧深度
    pub(crate) func_entry_frame_depth: usize,
    pub(crate) current_param_bindings: Option<Vec<(String, usize)>>,

    // ==================== 其他配置 ====================
    pub(crate) stdlib_path: Option<String>,
    pub(crate) args: Vec<String>,
    predefined_constants: HashMap<String, String>,

    // ==================== GC 相关 ====================
    /// 临时 GC 根（用于正在求值的值）
    pub(crate) gc_temp_roots: Vec<Value>,
    /// 需要 GC 保护的活动 VM 栈
    pub(crate) active_vm_stacks: Vec<*const Vec<Value>>,
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
            heap: crate::core::heap::Heap::new(),
            #[cfg(feature = "generational-gc")]
            gen_heap: crate::core::generational_heap::GenerationalHeap::new(),
            caps: capabilities::Capabilities::default(),
            output: String::new(),
            main_invoked: false,
            call_stack_depth: 0,
            rng_state: seed,
            config,
            // 类型系统
            types: TypeSystem::new(),
            // 缓存管理
            caches: CacheManager::new(),
            // 对象池
            pools: ObjectPools::new(),
            // 模块系统
            module_loader: Box::new(modules::StdModuleLoader),
            frontend: None,
            loaded_modules: fast_map_new(),
            import_parse_cache: fast_map_new(),
            import_stack: Vec::new(),
            entry_path: None,
            // 局部变量管理
            locals: slot_allocator::LocalSlots::new(),
            compiled_locals: fast_map_new(),
            compiled_locals_idx: fast_map_new(),
            current_func: None,
            func_entry_frame_depth: 0,
            current_param_bindings: None,
            // 其他配置
            stdlib_path: None,
            args: Vec::new(),
            predefined_constants: fast_map_new(),
            // GC 相关
            gc_temp_roots: Vec::new(),
            active_vm_stacks: Vec::new(),
        };
        rt.install_builtins();
        rt
    }

    /// Allocate an object on the heap.
    /// When generational-gc feature is enabled, allocates on gen_heap.
    /// Otherwise, allocates on the original heap.
    #[inline]
    #[cfg(not(feature = "generational-gc"))]
    pub fn alloc(&mut self, obj: crate::core::heap::ManagedObject) -> crate::core::heap::ObjectId {
        self.heap.alloc(obj)
    }

    /// Allocate an object on the heap (generational GC version).
    /// Allocates on the original heap and tracks allocation count.
    #[inline]
    #[cfg(feature = "generational-gc")]
    pub fn alloc(&mut self, obj: crate::core::heap::ManagedObject) -> crate::core::heap::ObjectId {
        let id = self.heap.alloc(obj);
        self.gen_heap.track_alloc(id);
        id
    }

    /// Write barrier for generational GC.
    /// Call this when storing a reference into a container object.
    #[inline]
    #[cfg(feature = "generational-gc")]
    pub fn write_barrier(&mut self, container_id: crate::core::heap::ObjectId) {
        self.gen_heap.write_barrier(container_id.0);
    }

    /// Write barrier (no-op when generational GC is disabled)
    #[inline]
    #[cfg(not(feature = "generational-gc"))]
    pub fn write_barrier(&mut self, _container_id: crate::core::heap::ObjectId) {
        // No-op
    }

    /// Get mutable reference to heap object with automatic write barrier.
    /// Use this instead of heap.get_mut() when modifying container objects.
    #[inline]
    #[cfg(feature = "generational-gc")]
    pub fn heap_get_mut(&mut self, id: crate::core::heap::ObjectId) -> &mut crate::core::heap::ManagedObject {
        self.gen_heap.write_barrier(id.0);
        self.heap.get_mut(id)
    }

    /// Get mutable reference to heap object (no write barrier needed).
    #[inline]
    #[cfg(not(feature = "generational-gc"))]
    pub fn heap_get_mut(&mut self, id: crate::core::heap::ObjectId) -> &mut crate::core::heap::ManagedObject {
        self.heap.get_mut(id)
    }

    pub fn intern_string(&mut self, s: &str) -> Text {
        if s.len() <= 22 {
            // Text::INLINE_CAP
            return Text::from_str(s);
        }
        if let Some(rc) = self.caches.string_pool.get(s) {
            return Text::Heap { data: rc.clone(), char_count: std::cell::Cell::new(u32::MAX) };
        }
        let rc = Rc::new(s.to_string());
        self.caches.string_pool.insert(s.to_string(), rc.clone());
        Text::Heap { data: rc, char_count: std::cell::Cell::new(u32::MAX) }
    }

    /// Get or create a pre-allocated string Value for a bytecode constant.
    /// Uses bytecode pointer + constant index as cache key.
    #[inline]
    pub fn get_string_const(&mut self, bc_ptr: usize, idx: u32, s: &str) -> Value {
        // Fast path: check if we have a cached value
        if let Some(cache) = self.caches.bytecode_string_cache.get(&bc_ptr) {
            if let Some(Some(val)) = cache.get(idx as usize) {
                return *val;
            }
        }
        // Slow path: create and cache the value
        let text = self.intern_string(s);
        let val = Value::str(self.alloc(crate::core::heap::ManagedObject::Str(text)));

        let cache = self.caches.bytecode_string_cache.entry(bc_ptr).or_default();
        let idx_usize = idx as usize;
        if cache.len() <= idx_usize {
            cache.resize(idx_usize + 1, None);
        }
        cache[idx_usize] = Some(val);
        val
    }

    pub(crate) fn option_none(&mut self) -> Value {
        if let Some(v) = self.caches.cached_option_none {
            return v;
        }
        static OPTION: &str = "Option";
        static NONE: &str = "none";
        let v = Value::enum_obj(self.alloc(crate::core::heap::ManagedObject::Enum(Box::new((
            crate::Text::from_str(OPTION),
            crate::Text::from_str(NONE),
            Box::new([]),
        )))));
        self.caches.cached_option_none = Some(v);
        v
    }

    /// Get cached string Value for small integers (0-99999)
    /// Returns None if the integer is out of range
    /// Uses lazy initialization to avoid upfront memory allocation
    #[inline]
    pub fn get_small_int_string(&mut self, i: i64) -> Option<Value> {
        // Cache integers 0-99999 (100K) - balances performance and memory
        // This covers most common use cases while keeping memory reasonable
        const MAX_CACHED: usize = 100_000;
        if i < 0 || i >= MAX_CACHED as i64 {
            return None;
        }
        let idx = i as usize;
        // Ensure cache is large enough (lazy resize)
        if self.caches.small_int_strings.len() <= idx {
            self.caches.small_int_strings.resize(idx + 1, None);
        }
        // Return cached value or create new one
        if let Some(v) = self.caches.small_int_strings[idx] {
            Some(v)
        } else {
            let text = crate::core::value::i64_to_text_fast(i);
            let v = Value::str(self.alloc(crate::core::heap::ManagedObject::Str(text)));
            self.caches.small_int_strings[idx] = Some(v);
            Some(v)
        }
    }

    /// Intern a string value - returns cached Value if the string was seen before.
    /// This is useful for operations like split() that may produce many duplicate strings.
    /// Only interns strings <= 64 bytes to avoid unbounded cache growth.
    /// Cache is limited to MAX_INTERN_SIZE entries to prevent memory accumulation.
    #[inline]
    pub fn intern_str_value(&mut self, s: &str) -> Value {
        // Only intern short strings to limit cache size
        const MAX_INTERN_LEN: usize = 64;
        const MAX_INTERN_SIZE: usize = 100_000;  // 提高到10万以减少性能影响

        if s.len() > MAX_INTERN_LEN {
            // Don't intern long strings - just create directly
            let text = crate::core::text::Text::from_str(s);
            return Value::str(self.alloc(crate::core::heap::ManagedObject::Str(text)));
        }

        // Limit cache size - clear half when full
        if self.caches.string_value_intern.len() >= MAX_INTERN_SIZE {
            let to_remove = MAX_INTERN_SIZE / 2;
            let keys: Vec<_> = self.caches.string_value_intern
                .keys()
                .take(to_remove)
                .cloned()
                .collect();
            for k in keys {
                self.caches.string_value_intern.swap_remove(&k);
            }
        }

        // Compute hash for lookup
        let hash = {
            use std::hash::{BuildHasher, Hasher};
            let mut h = self.caches.string_value_intern.hasher().build_hasher();
            h.write(s.as_bytes());
            h.finish()
        };

        // Check cache
        if let Some(&val) = self.caches.string_value_intern.get(&hash) {
            // Verify it's actually the same string (hash collision check)
            if let crate::core::heap::ManagedObject::Str(cached_text) = self.heap.get(val.as_obj_id()) {
                if cached_text.as_str() == s {
                    return val;
                }
            }
        }

        // Create new string and cache it
        let text = self.intern_string(s);
        let val = Value::str(self.alloc(crate::core::heap::ManagedObject::Str(text)));
        self.caches.string_value_intern.insert(hash, val);
        val
    }

    pub(crate) fn option_some(&mut self, v: Value) -> Value {
        // Use optimized OptionSome variant to avoid Box allocation
        Value::option_some(self.alloc(crate::core::heap::ManagedObject::OptionSome(v)))
    }

    /// Get a String from the builder pool, or create a new one with the given capacity
    pub(crate) fn builder_pool_get(&mut self, cap: usize) -> String {
        self.pools.get_builder(cap)
    }

    /// Return a String to the builder pool for reuse
    pub(crate) fn builder_pool_return(&mut self, s: String) {
        self.pools.return_builder(s);
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

    pub(crate) fn dict_get_by_str_with_hash(me: &Dict, key: &str, _key_hash: u64) -> Option<Value> {
        let dict_key_hash = DictKey::hash_str(key);
        // Compute HashMap hash from DictKey hash
        use std::hash::{BuildHasher, Hasher};
        let mut h = me.map.hasher().build_hasher();
        h.write_u8(0); // String discriminant
        h.write_u64(dict_key_hash);
        let hash = h.finish();

        me.map
            .raw_entry_v1()
            .from_hash(hash, |k| {
                // Compare by hash only - hash collision is rare
                if let DictKey::StrRef { hash: kh, .. } = k {
                    *kh == dict_key_hash
                } else {
                    false
                }
            })
            .map(|(_, v)| *v)
    }

    pub(crate) fn enum_new_checked(
        &mut self,
        ty: &str,
        variant: &str,
        payload: Box<[Value]>,
    ) -> Result<Value, String> {
        if let Some(vars) = self.types.enums.get(ty) {
            if !vars.contains(&variant.to_string()) {
                return Err(self.error(xu_syntax::DiagnosticKind::UnknownEnumVariant(
                    ty.to_string(),
                    variant.to_string(),
                )));
            }
        }
        // Optimize Option#some to use TAG_OPTION
        if ty == "Option" && variant == "some" && payload.len() == 1 {
            return Ok(self.option_some(payload[0]));
        }
        let id = self.alloc(crate::core::heap::ManagedObject::Enum(Box::new((
            ty.to_string().into(),
            variant.to_string().into(),
            payload,
        ))));
        Ok(Value::enum_obj(id))
    }

    pub(crate) fn enum_parts_cloned(
        &self,
        v: Value,
    ) -> Result<(crate::Text, crate::Text, Box<[Value]>), String> {
        if v.get_tag() != crate::core::value::TAG_ENUM {
            return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-enum object".into())));
        }
        match self.heap.get(v.as_obj_id()) {
            crate::core::heap::ManagedObject::Enum(e) => {
                let (ty, variant, payload) = e.as_ref();
                Ok((
                    ty.clone(),
                    variant.clone(),
                    payload.to_vec().into_boxed_slice(),
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

    pub fn set_module_loader(&mut self, loader: Box<dyn modules::ModuleLoader>) {
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

    // ==================== 辅助方法 ====================

    /// 处理执行流程结果
    fn handle_exec_flow(&mut self, flow: Flow) -> Result<ExecResult, String> {
        match flow {
            Flow::None => {
                self.invoke_main_if_present()?;
                Ok(ExecResult { value: None, output: std::mem::take(&mut self.output) })
            }
            Flow::Return(v) => Ok(ExecResult { value: Some(v), output: std::mem::take(&mut self.output) }),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => Err(self.error(xu_syntax::DiagnosticKind::TopLevelBreakContinue)),
        }
    }

    pub fn exec_module(&mut self, module: &Module) -> Result<ExecResult, String> {
        self.reset_for_entry_execution();
        self.compiled_locals = Self::collect_func_locals(module);
        self.compiled_locals_idx = Self::index_func_locals(&self.compiled_locals);
        Self::precompile_module(module)?;
        let flow = self.exec_stmts(&module.stmts);
        self.handle_exec_flow(flow)
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
                Stmt::StructDef(def) => { self.types.structs.insert(def.name.clone(), (**def).clone()); }
                Stmt::EnumDef(def) => { self.types.enums.insert(def.name.clone(), def.variants.to_vec()); }
                _ => {}
            }
        }
        Self::precompile_module(&program.module)?;
        let flow = if let Some(bc) = program.bytecode.as_ref() {
            ir::run_bytecode(self, bc)?
        } else {
            self.exec_stmts(&program.module.stmts)
        };
        self.handle_exec_flow(flow)
    }

    pub(crate) fn reset_for_entry_execution(&mut self) {
        self.output.clear();
        self.main_invoked = false;
        self.import_stack.clear();
        self.loaded_modules.clear();
        self.types.reset();
        self.locals.clear();

        self.env = Env::new();
        self.heap = crate::core::heap::Heap::new();
        self.install_builtins();
        for (k, v) in &self.predefined_constants {
            let s = self
                .heap
                .alloc(crate::core::heap::ManagedObject::Str(v.to_string().into()));
            self.env.define(k.clone(), Value::str(s));
        }
        self.caches.reset();
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
        if v.get_tag() == crate::core::value::TAG_FUNC {
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

    pub(crate) fn clock_unix_secs(&self) -> i64 { self.caps.clock.unix_secs() }
    pub(crate) fn clock_unix_millis(&self) -> i64 { self.caps.clock.unix_millis() }
    pub(crate) fn clock_mono_micros(&self) -> i64 { self.caps.clock.mono_micros() }
    pub(crate) fn clock_mono_nanos(&self) -> i64 { self.caps.clock.mono_nanos() }

    pub(crate) fn fs_metadata(&self, path: &str) -> Result<(), String> {
        self.caps.fs.metadata(path).map_err(|e| format!("Open failed: {e}"))
    }

    pub(crate) fn fs_read_to_string(&self, path: &str) -> Result<String, String> {
        self.caps.fs.read_to_string(path).map_err(|e| format!("Read failed: {e}"))
    }

    pub(crate) fn fs_read_to_string_import(&self, path: &str) -> Result<String, String> {
        self.caps.fs.read_to_string(path).map_err(|e| format!("Import failed: {e}"))
    }

    pub(crate) fn fs_stat(&self, path: &str) -> Result<capabilities::FileStat, String> {
        self.caps.fs.stat(path).map_err(|e| format!("Import failed: {e}"))
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
                        // Only collect variables that are declared with let/var
                        // (indicated by a.decl being Some)
                        if a.decl.is_some() {
                            if let Expr::Ident(n, _) = &a.target {
                                push_unique(ordered, seen, n);
                            }
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

    pub(crate) fn format_throw(&self, v: &Value) -> String {
        if v.get_tag() == crate::core::value::TAG_STR {
            if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(v.as_obj_id()) {
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
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

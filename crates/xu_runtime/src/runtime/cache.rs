//! Inline cache types for runtime optimization.

use crate::core::Value;
use crate::core::text::Text;
use crate::methods::MethodKind;

/// Inline cache slot for property/dict access optimization.
#[derive(Clone, Default)]
pub struct ICSlot {
    pub id: usize,
    pub key_hash: u64,
    pub key_id: usize,
    pub key_short: [u8; 16],
    pub key_len: u8,
    pub ver: u64,
    pub value: Value,
    pub option_some_cached: Value,
    pub struct_ty_hash: u64,
    pub field_offset: Option<usize>,
}

/// Inline cache slot for method call optimization.
#[derive(Clone, Default)]
pub struct MethodICSlot {
    pub tag: u64,
    pub method_hash: u64,
    pub struct_ty_hash: u64,
    pub(crate) kind: MethodKind,
    pub cached_func: Value,
    pub cached_user: Option<std::rc::Rc<crate::core::value::UserFunction>>,
    pub cached_bytecode: Option<std::rc::Rc<crate::core::value::BytecodeFunction>>,
}

/// Last dict cache entry for string keys.
#[derive(Clone)]
pub(crate) struct DictCacheLast {
    pub(crate) id: usize,
    pub(crate) ver: u64,
    pub(crate) key: Text,
    pub(crate) value: Value,
}

/// Last dict insert cache for hot key optimization.
#[derive(Clone)]
pub(crate) struct DictInsertCacheLast {
    pub(crate) dict_id: usize,
    pub(crate) key_obj_id: usize,
    pub(crate) key_hash: u64,
    pub(crate) map_hash: u64,
    #[allow(dead_code)]
    pub(crate) map_index: Option<usize>,  // Cached index in the map (reserved for future use)
    #[allow(dead_code)]
    pub(crate) dict_ver: u64,             // Dict version when index was cached
}

/// Last dict cache entry for integer keys.
#[derive(Clone)]
pub(crate) struct DictCacheIntLast {
    pub(crate) id: usize,
    pub(crate) key: i64,
    pub(crate) ver: u64,
    pub(crate) value: Value,
}

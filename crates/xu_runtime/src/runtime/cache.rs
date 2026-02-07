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
///
/// 用于优化重复更新同一个键的场景（如循环中的 `d[key] = value`）。
/// 当检测到相同的字典和键对象时，跳过哈希计算直接使用缓存的哈希值。
#[derive(Clone)]
pub(crate) struct DictInsertCacheLast {
    /// 字典对象 ID
    pub(crate) dict_id: usize,
    /// 键字符串对象 ID
    pub(crate) key_obj_id: usize,
    /// DictKey 的哈希值
    pub(crate) key_hash: u64,
    /// IndexMap 的哈希值
    pub(crate) map_hash: u64,
}

/// Last dict cache entry for integer keys.
#[derive(Clone)]
pub(crate) struct DictCacheIntLast {
    pub(crate) id: usize,
    pub(crate) key: i64,
    pub(crate) ver: u64,
    pub(crate) value: Value,
}

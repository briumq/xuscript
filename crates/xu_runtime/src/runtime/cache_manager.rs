//! 缓存管理子模块
//!
//! 管理各种运行时缓存，包括方法缓存、字典缓存、IC 槽等。

use std::rc::Rc;
use crate::core::Value;
use crate::core::value::{FastHashMap, fast_map_new};
use super::cache::{ICSlot, MethodICSlot, DictCacheLast, DictCacheIntLast};

type HashMap<K, V> = FastHashMap<K, V>;

/// 缓存管理器
///
/// 包含所有缓存相关的字段：
/// - method_cache: 方法查找缓存
/// - dict_cache_*: 字典操作缓存
/// - ic_slots: 内联缓存槽
/// - string_pool: 字符串池
/// - bytecode_string_cache: 字节码字符串缓存
/// - small_int_strings: 小整数字符串缓存
pub struct CacheManager {
    /// 方法查找缓存: (type_name, method_name) -> Value
    pub method_cache: HashMap<(String, String), Value>,
    /// 字典字符串键缓存
    pub dict_cache_last: Option<DictCacheLast>,
    /// 字典整数键缓存
    pub dict_cache_int_last: Option<DictCacheIntLast>,
    /// 字典版本缓存
    pub dict_version_last: Option<(usize, u64)>,
    /// 内联缓存槽
    pub ic_slots: Vec<ICSlot>,
    /// 方法内联缓存槽
    pub ic_method_slots: Vec<MethodICSlot>,
    /// 字符串池（用于字符串驻留）
    pub string_pool: HashMap<String, Rc<String>>,
    /// 字节码字符串常量缓存
    /// 键为字节码指针，值为常量索引到预分配 Value 的映射
    pub bytecode_string_cache: HashMap<usize, Vec<Option<Value>>>,
    /// 小整数字符串缓存 (0-499999)
    pub small_int_strings: Vec<Option<Value>>,
    /// 缓存的 Option::none 值
    pub cached_option_none: Option<Value>,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new() -> Self {
        Self {
            method_cache: fast_map_new(),
            dict_cache_last: None,
            dict_cache_int_last: None,
            dict_version_last: None,
            ic_slots: Vec::new(),
            ic_method_slots: Vec::new(),
            string_pool: fast_map_new(),
            bytecode_string_cache: fast_map_new(),
            small_int_strings: Vec::new(),
            cached_option_none: None,
        }
    }

    /// 重置缓存状态（用于新的执行）
    pub fn reset(&mut self) {
        self.method_cache.clear();
        self.dict_cache_last = None;
        self.dict_cache_int_last = None;
        self.dict_version_last = None;
        self.ic_slots.clear();
        self.ic_method_slots.clear();
        // 注意：string_pool, bytecode_string_cache, small_int_strings 不重置
        // 因为它们可以跨执行复用
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
}

//! Dict 操作辅助函数
//!
//! 提供字典操作的公共函数，用于消除代码重复：
//! - `compute_map_hash`: 计算字符串键在 IndexMap 中的哈希值
//! - `update_hot_key_cache`: 更新热点键缓存

use std::hash::{BuildHasher, Hasher};

use super::DictInsertCacheLast;

/// 计算字符串键在 IndexMap 中的哈希值
///
/// 使用 DictKey 的哈希值（而非字符串字节）来计算 IndexMap 的哈希值。
/// 这与 DictKey 的 Hash 实现保持一致。
///
/// # 参数
/// - `hasher`: IndexMap 的 BuildHasher
/// - `dict_key_hash`: DictKey::hash_str() 计算的哈希值
///
/// # 返回
/// IndexMap 使用的哈希值
#[inline]
pub fn compute_map_hash<S: BuildHasher>(hasher: &S, dict_key_hash: u64) -> u64 {
    let mut h = hasher.build_hasher();
    h.write_u8(0); // String discriminant
    h.write_u64(dict_key_hash);
    h.finish()
}

/// 更新热点键缓存
///
/// # 参数
/// - `cache`: 热点键缓存
/// - `dict_id`: 字典对象 ID
/// - `key_obj_id`: 键字符串对象 ID
/// - `key_hash`: DictKey 的哈希值
/// - `map_hash`: IndexMap 的哈希值
#[inline]
pub fn update_hot_key_cache(
    cache: &mut Option<DictInsertCacheLast>,
    dict_id: usize,
    key_obj_id: usize,
    key_hash: u64,
    map_hash: u64,
) {
    *cache = Some(DictInsertCacheLast {
        dict_id,
        key_obj_id,
        key_hash,
        map_hash,
    });
}

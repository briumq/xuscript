//! 类型检查共享逻辑
//! 此模块包含 ast_exec 和 vm 之间共享的类型检查代码。

use crate::core::heap::{Heap, ManagedObject};
use crate::core::value::{Value, TAG_STRUCT};

/// 类型签名计算的魔数（FNV-1a 初始值）
pub const TYPE_SIG_INIT: u64 = 1469598103934665603u64;

/// FNV-1a 乘数
pub const FNV_PRIME: u64 = 1099511628211;

/// 计算参数列表的类型签名
/// 用于类型检查的内联缓存（IC）优化。
/// 当类型签名匹配时，可以跳过类型检查。
#[inline]
pub fn compute_type_signature(args: &[Value], heap: &Heap) -> u64 {
    let mut type_sig = TYPE_SIG_INIT;
    for v in args {
        let mut x = v.get_tag() as u64;
        if v.get_tag() == TAG_STRUCT {
            let id = v.as_obj_id();
            if let ManagedObject::Struct(si) = heap.get(id) {
                x ^= si.ty_hash;
            }
        }
        type_sig ^= x;
        type_sig = type_sig.wrapping_mul(FNV_PRIME);
    }
    type_sig
}

/// 检查是否应该使用类型 IC
#[inline]
pub fn should_use_type_ic(params: &[xu_ir::Param], args_len: usize) -> bool {
    args_len == params.len()
        && params.iter().all(|p| p.default.is_none())
        && params.iter().any(|p| p.ty.is_some())
}

/// 检查类型签名是否匹配缓存
#[inline]
pub fn type_sig_matches(cached: Option<u64>, computed: u64) -> bool {
    cached == Some(computed)
}

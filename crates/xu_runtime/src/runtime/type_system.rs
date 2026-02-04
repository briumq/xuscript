//! 类型系统子模块
//!
//! 管理结构体、枚举、静态字段等类型相关的数据。

use xu_ir::StructDef;
use crate::core::Value;
use crate::core::value::{FastHashMap, fast_map_new};
use std::rc::Rc;

type HashMap<K, V> = FastHashMap<K, V>;

/// 类型系统管理器
///
/// 包含所有类型定义相关的字段：
/// - structs: 结构体定义
/// - struct_layouts: 结构体字段布局缓存
/// - enums: 枚举定义
/// - static_fields: 静态字段存储
/// - next_id: 下一个唯一 ID
pub struct TypeSystem {
    /// 结构体定义映射
    pub structs: HashMap<String, StructDef>,
    /// 结构体字段布局缓存
    pub struct_layouts: HashMap<String, Rc<[String]>>,
    /// 枚举定义映射
    pub enums: HashMap<String, Vec<String>>,
    /// 静态字段存储: (type_name, field_name) -> Value
    pub static_fields: HashMap<(String, String), Value>,
    /// 下一个唯一 ID
    pub next_id: i64,
}

impl TypeSystem {
    /// 创建新的类型系统
    pub fn new() -> Self {
        Self {
            structs: fast_map_new(),
            struct_layouts: fast_map_new(),
            enums: fast_map_new(),
            static_fields: fast_map_new(),
            next_id: 1,
        }
    }

    /// 重置类型系统状态
    pub fn reset(&mut self) {
        self.structs.clear();
        self.struct_layouts.clear();
        self.enums.clear();
        self.static_fields.clear();
        self.next_id = 1;
    }
}

impl Default for TypeSystem {
    fn default() -> Self {
        Self::new()
    }
}

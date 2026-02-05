//! Runtime value representation.
//!
//! Defines the runtime value representation using NaN-boxing for efficient memory usage.

use super::heap::{Heap, ManagedObject, ObjectId};
use super::text::Text;
use crate::errors::messages::NOT_A_STRING;
use ahash::RandomState;
use hashbrown::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use xu_ir::{Bytecode, FuncDef, BinaryOp};

// ============================================================================
// Constants
// ============================================================================

/// Maximum integer key to store in elements array (0 to ELEMENTS_MAX-1).
/// Keys in this range use O(1) array lookup instead of hash map.
/// Increased from 1024 to 65536 for better performance with larger integer keys.
pub const ELEMENTS_MAX: i64 = 65536;

// ============================================================================
// Dictionary key types
// ============================================================================

/// Inline capacity for short string keys (same as Text)
const DICT_KEY_INLINE_CAP: usize = 22;

/// Compact dict key representation with small string optimization.
/// Short strings (<=22 bytes) are stored inline to avoid heap allocation.
#[derive(Clone)]
pub enum DictKey {
    /// Inline string storage for short keys (no heap allocation)
    StrInline { hash: u64, len: u8, buf: [u8; DICT_KEY_INLINE_CAP] },
    /// Heap-allocated string for longer keys
    Str { hash: u64, data: Rc<String> },
    /// Integer key
    Int(i64),
}

impl fmt::Debug for DictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictKey::StrInline { hash, len, buf } => {
                f.debug_struct("StrInline").field("hash", hash).field("data", &Self::inline_str(*len, buf)).finish()
            }
            DictKey::Str { hash, data } => f.debug_struct("Str").field("hash", hash).field("data", data).finish(),
            DictKey::Int(i) => f.debug_tuple("Int").field(i).finish(),
        }
    }
}

impl PartialEq for DictKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DictKey::StrInline { hash: h1, len: l1, buf: b1 }, DictKey::StrInline { hash: h2, len: l2, buf: b2 }) => {
                h1 == h2 && l1 == l2 && b1[..*l1 as usize] == b2[..*l2 as usize]
            }
            (DictKey::StrInline { hash: h1, len, buf }, DictKey::Str { hash: h2, data }) |
            (DictKey::Str { hash: h2, data }, DictKey::StrInline { hash: h1, len, buf }) => {
                h1 == h2 && Self::inline_str(*len, buf) == data.as_str()
            }
            (DictKey::Str { hash: h1, data: d1 }, DictKey::Str { hash: h2, data: d2 }) => {
                h1 == h2 && (Rc::ptr_eq(d1, d2) || d1.as_str() == d2.as_str())
            }
            (DictKey::Int(a), DictKey::Int(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for DictKey {}

impl DictKey {
    pub fn is_str(&self) -> bool {
        matches!(self, DictKey::Str { .. } | DictKey::StrInline { .. })
    }

    /// 获取内联字符串的切片
    #[inline]
    fn inline_str(len: u8, buf: &[u8; DICT_KEY_INLINE_CAP]) -> &str {
        unsafe { std::str::from_utf8_unchecked(&buf[..len as usize]) }
    }

    /// Create a new string key with pre-computed hash
    /// Uses inline storage for short strings to avoid heap allocation
    #[inline]
    pub fn from_str(s: &str) -> Self {
        let hash = Self::hash_str(s);
        if s.len() <= DICT_KEY_INLINE_CAP {
            let mut buf = [0u8; DICT_KEY_INLINE_CAP];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            DictKey::StrInline { hash, len: s.len() as u8, buf }
        } else {
            DictKey::Str { hash, data: Rc::new(s.to_string()) }
        }
    }

    /// Create a new string key from Rc<String> with pre-computed hash
    pub fn from_rc(data: Rc<String>) -> Self {
        DictKey::Str { hash: Self::hash_str(&data), data }
    }

    /// Create a new string key from Text
    pub fn from_text(t: &Text) -> Self {
        Self::from_str(t.as_str())
    }

    /// Compute hash for a string (used for fast equality comparison)
    #[inline]
    pub fn hash_str(s: &str) -> u64 {
        use std::hash::Hasher;
        let mut hasher = ahash::AHasher::default();
        hasher.write(s.as_bytes());
        hasher.finish()
    }

    /// Get the string content (panics if not a string key)
    pub fn as_str(&self) -> &str {
        match self {
            DictKey::StrInline { len, buf, .. } => Self::inline_str(*len, buf),
            DictKey::Str { data, .. } => data.as_str(),
            DictKey::Int(_) => panic!("DictKey::as_str called on Int"),
        }
    }
}

impl Hash for DictKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            DictKey::StrInline { len, buf, .. } => { state.write_u8(0); buf[..*len as usize].hash(state); }
            DictKey::Str { data, .. } => { state.write_u8(0); data.as_bytes().hash(state); }
            DictKey::Int(i) => { state.write_u8(1); i.hash(state); }
        }
    }
}

impl fmt::Display for DictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictKey::StrInline { len, buf, .. } => write!(f, "{}", Self::inline_str(*len, buf)),
            DictKey::Str { data, .. } => write!(f, "{}", data),
            DictKey::Int(i) => write!(f, "{}", i),
        }
    }
}

// ============================================================================
// HashMap and Dict types
// ============================================================================

pub type FastHashMap<K, V> = HashMap<K, V, RandomState>;

pub struct DictInstance {
    pub map: FastHashMap<DictKey, Value>,
    pub elements: Vec<Value>,
    pub shape: Option<ObjectId>,
    pub prop_values: Vec<Value>,
    pub ver: u64,
}

impl Clone for DictInstance {
    fn clone(&self) -> Self {
        let mut map = fast_map_with_capacity(self.map.len());
        for (k, v) in self.map.iter() {
            map.insert(k.clone(), v.clone());
        }
        Self {
            map,
            elements: self.elements.clone(),
            shape: self.shape,
            prop_values: self.prop_values.clone(),
            ver: self.ver,
        }
    }
}

#[derive(Clone)]
pub struct Shape {
    pub parent: Option<ObjectId>,
    pub prop_map: FastHashMap<String, usize>,
    pub transitions: FastHashMap<String, ObjectId>,
}

pub type Dict = Box<DictInstance>;

pub struct DictStrInstance {
    pub map: FastHashMap<String, Value>,
    pub ver: u64,
}

impl Clone for DictStrInstance {
    fn clone(&self) -> Self {
        let mut map = fast_map_with_capacity(self.map.len());
        for (k, v) in self.map.iter() {
            map.insert(k.clone(), v.clone());
        }
        Self { map, ver: self.ver }
    }
}

pub type DictStr = Box<DictStrInstance>;

pub fn fast_hasher() -> RandomState {
    RandomState::with_seeds(0, 0, 0, 0)
}

pub fn fast_map_new<K: Eq + Hash, V>() -> FastHashMap<K, V> {
    HashMap::with_hasher(fast_hasher())
}

pub fn fast_map_with_capacity<K: Eq + Hash, V>(cap: usize) -> FastHashMap<K, V> {
    HashMap::with_capacity_and_hasher(cap, fast_hasher())
}

pub fn dict_with_capacity(cap: usize) -> Dict {
    Box::new(DictInstance {
        map: fast_map_with_capacity(cap),
        elements: Vec::new(),
        shape: None,
        prop_values: Vec::new(),
        ver: 0,
    })
}

pub fn dict_str_new() -> DictStr {
    Box::new(DictStrInstance {
        map: fast_map_new(),
        ver: 0,
    })
}

// ============================================================================
// Integer to string conversion helpers
// ============================================================================

use super::text::write_i64_to_buf;

pub fn i64_to_string_fast(i: i64) -> String {
    let mut buf = [0u8; 32];
    let digits = write_i64_to_buf(i, &mut buf);
    let s = unsafe { std::str::from_utf8_unchecked(digits) };
    let mut out = String::with_capacity(s.len());
    out.push_str(s);
    out
}

pub fn i64_to_text_fast(i: i64) -> Text {
    let mut buf = [0u8; 32];
    let digits = write_i64_to_buf(i, &mut buf);
    let s = unsafe { std::str::from_utf8_unchecked(digits) };
    Text::from_str(s)
}

// ============================================================================
// NaN-Boxing constants and Value type
// ============================================================================

pub const QNAN: u64 = 0x7ff8000000000000;
pub const TAG_BASE: u64 = 0xfff0000000000000;
pub const TAG_MASK: u64 = 0x000f000000000000;
pub const PAYLOAD_MASK: u64 = 0x0000ffffffffffff;

pub const TAG_INT: u64 = 0x0001;
pub const TAG_BOOL: u64 = 0x0002;
pub const TAG_UNIT: u64 = 0x0003;

pub const TAG_LIST: u64 = 0x0004;
pub const TAG_DICT: u64 = 0x0005;
pub const TAG_STR: u64 = 0x0006;
pub const TAG_STRUCT: u64 = 0x0007;
pub const TAG_MODULE: u64 = 0x0008;
pub const TAG_FUNC: u64 = 0x0009;
pub const TAG_FILE: u64 = 0x000a;
pub const TAG_RANGE: u64 = 0x000b;
pub const TAG_ENUM: u64 = 0x000c;
pub const TAG_BUILDER: u64 = 0x000d;
pub const TAG_TUPLE: u64 = 0x000e;
pub const TAG_OPTION: u64 = 0x000f;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Value(u64);

impl Default for Value {
    fn default() -> Self {
        Self::UNIT
    }
}

impl Value {
    pub const UNIT: Value = Value(TAG_BASE | (TAG_UNIT << 48));

    pub fn none() -> Self {
        Self::UNIT
    }

    pub fn some(id: ObjectId) -> Self {
        Self::from_obj(TAG_OPTION, id)
    }

    #[inline(always)]
    pub fn from_f64(f: f64) -> Self {
        // If it's a NaN, we normalize it to a specific NaN pattern to avoid conflict with tags
        if f.is_nan() {
            return Self(QNAN);
        }
        Self(f.to_bits())
    }

    #[inline(always)]
    pub fn from_i64(i: i64) -> Self {
        // Truncate to 48 bits for now.
        // In a real implementation, we might box larger ones.
        Self(TAG_BASE | (TAG_INT << 48) | (i as u64 & PAYLOAD_MASK))
    }

    #[inline(always)]
    pub fn from_bool(b: bool) -> Self {
        Self(TAG_BASE | (TAG_BOOL << 48) | (if b { 1 } else { 0 }))
    }

    #[inline(always)]
    fn from_obj(tag: u64, id: ObjectId) -> Self {
        Self(TAG_BASE | (tag << 48) | (id.0 as u64 & PAYLOAD_MASK))
    }

    pub fn list(id: ObjectId) -> Self {
        Self::from_obj(TAG_LIST, id)
    }
    pub fn dict(id: ObjectId) -> Self {
        Self::from_obj(TAG_DICT, id)
    }
    pub fn str(id: ObjectId) -> Self {
        Self::from_obj(TAG_STR, id)
    }
    pub fn tuple(id: ObjectId) -> Self {
        Self::from_obj(TAG_TUPLE, id)
    }
    pub fn struct_obj(id: ObjectId) -> Self {
        Self::from_obj(TAG_STRUCT, id)
    }
    pub fn module(id: ObjectId) -> Self {
        Self::from_obj(TAG_MODULE, id)
    }
    pub fn function(id: ObjectId) -> Self {
        Self::from_obj(TAG_FUNC, id)
    }
    pub fn file(id: ObjectId) -> Self {
        Self::from_obj(TAG_FILE, id)
    }
    pub fn range(id: ObjectId) -> Self {
        Self::from_obj(TAG_RANGE, id)
    }
    pub fn enum_obj(id: ObjectId) -> Self {
        Self::from_obj(TAG_ENUM, id)
    }
    pub fn builder(id: ObjectId) -> Self {
        Self::from_obj(TAG_BUILDER, id)
    }
    pub fn option_some(id: ObjectId) -> Self {
        Self::from_obj(TAG_OPTION, id)
    }

    #[inline(always)]
    pub fn is_f64(&self) -> bool {
        (self.0 & TAG_BASE) != TAG_BASE
    }
    #[inline(always)]
    pub fn is_int(&self) -> bool {
        (self.0 & 0xffff000000000000) == 0xfff1000000000000
    }
    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        !self.is_f64() && self.get_tag() == TAG_BOOL
    }
    #[inline(always)]
    pub fn is_unit(&self) -> bool {
        !self.is_f64() && self.get_tag() == TAG_UNIT
    }
    #[inline(always)]
    pub fn is_obj(&self) -> bool {
        !self.is_f64() && self.get_tag() > TAG_UNIT
    }

    #[inline(always)]
    pub fn as_f64(self) -> f64 {
        f64::from_bits(self.0)
    }

    #[inline(always)]
    pub fn as_i64(&self) -> i64 {
        let val = (self.0 & PAYLOAD_MASK) as i64;
        // Sign extend from 48 bits
        if (val & 0x0000800000000000) != 0 {
            val | -0x0001000000000000
        } else {
            val
        }
    }

    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        (self.0 & 1) != 0
    }

    #[inline(always)]
    pub fn as_obj_id(&self) -> ObjectId {
        ObjectId((self.0 & PAYLOAD_MASK) as usize)
    }

    pub fn get_tag(&self) -> u64 {
        if self.is_f64() {
            0
        } else {
            (self.0 & TAG_MASK) >> 48
        }
    }

    pub fn type_name(&self) -> &'static str {
        if self.is_f64() {
            "float"
        } else if self.is_int() {
            "int"
        } else if self.is_bool() {
            "bool"
        } else if self.is_unit() {
            "unit"
        } else {
            let tag = self.get_tag();
            match tag {
                TAG_LIST => "list",
                TAG_DICT => "dict",
                TAG_STR => "string",
                TAG_STRUCT => "struct",
                TAG_MODULE => "module",
                TAG_FUNC => "function",
                TAG_FILE => "file",
                TAG_RANGE => "range",
                TAG_ENUM => "enum",
                TAG_BUILDER => "builder",
                TAG_TUPLE => "tuple",
                _ => "unknown",
            }
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_f64() {
            write!(f, "Float({})", self.as_f64())
        } else if self.is_int() {
            write!(f, "Int({})", self.as_i64())
        } else if self.is_bool() {
            write!(f, "Bool({})", self.as_bool())
        } else if self.is_unit() {
            write!(f, "Unit")
        } else {
            let tag = self.get_tag();
            let id = self.as_obj_id();
            match tag {
                TAG_LIST => write!(f, "List(id={:?})", id),
                TAG_DICT => write!(f, "Dict(id={:?})", id),
                TAG_STR => write!(f, "Str(id={:?})", id),
                TAG_STRUCT => write!(f, "Struct(id={:?})", id),
                TAG_MODULE => write!(f, "Module(id={:?})", id),
                TAG_FUNC => write!(f, "Function(id={:?})", id),
                TAG_FILE => write!(f, "File(id={:?})", id),
                TAG_RANGE => write!(f, "Range(id={:?})", id),
                TAG_ENUM => write!(f, "Enum(id={:?})", id),
                TAG_BUILDER => write!(f, "Builder(id={:?})", id),
                _ => write!(f, "Unknown(tag={}, id={:?})", tag, id),
            }
        }
    }
}

// ============================================================================
// Supporting instance types
// ============================================================================

#[derive(Clone)]
pub struct ModuleInstance {
    pub exports: DictStr,
}

#[derive(Clone)]
pub struct StructInstance {
    pub ty: String,
    pub ty_hash: u64,
    pub fields: Box<[Value]>,
    pub field_names: std::rc::Rc<[String]>,
}

#[derive(Clone)]
pub struct FileHandle {
    pub path: String,
    pub open: bool,
    pub content: String,
}

// ============================================================================
// Runtime-specific function types
// ============================================================================

#[derive(Clone)]
pub enum Function {
    User(Rc<UserFunction>),
    Builtin(BuiltinFunction),
    Bytecode(Rc<BytecodeFunction>),
}

#[derive(Clone)]
pub struct UserFunction {
    pub def: FuncDef,
    pub env: super::Env,
    pub needs_env_frame: bool,
    pub fast_param_indices: Option<Box<[usize]>>,
    pub fast_locals_size: Option<usize>,
    pub skip_local_map: bool,
    pub type_sig_ic: std::cell::Cell<Option<u64>>,
}

#[derive(Clone)]
pub struct BytecodeFunction {
    pub def: FuncDef,
    pub bytecode: Rc<Bytecode>,
    pub env: super::Env,
    pub needs_env_frame: bool,
    pub locals_count: usize,
    pub type_sig_ic: std::cell::Cell<Option<u64>>,
}

pub type BuiltinFunction = fn(&mut crate::Runtime, &[Value]) -> Result<Value, String>;

// ============================================================================
// ValueExt trait for heap-dependent operations
// ============================================================================

/// Extension trait for Value with heap-dependent and runtime-specific methods.
pub trait ValueExt {
    fn mark_into(&self, heap: &mut Heap, pending: &mut Vec<ObjectId>);
    fn bin_op(&self, op: BinaryOp, other: Value) -> Result<Value, String>;
    fn bin_op_assign(&mut self, op: BinaryOp, other: Value, heap: &mut Heap) -> Result<(), String>;
    fn to_string_lossy(&self, heap: &Heap) -> String;
}

impl ValueExt for Value {
    fn mark_into(&self, heap: &mut Heap, pending: &mut Vec<ObjectId>) {
        if self.is_obj() {
            let id = self.as_obj_id();
            if !heap.is_marked(id) {
                pending.push(id);
            }
        }
    }

    fn bin_op(&self, op: BinaryOp, other: Value) -> Result<Value, String> {
        match op {
            BinaryOp::Add => add(*self, other),
            BinaryOp::Sub => sub(*self, other),
            BinaryOp::Mul => mul(*self, other),
            BinaryOp::Div => div(*self, other),
            BinaryOp::Mod => rem(*self, other),
            BinaryOp::Eq => Ok(Value::from_bool(self == &other)),
            BinaryOp::Ne => Ok(Value::from_bool(self != &other)),
            BinaryOp::And => and(*self, other),
            BinaryOp::Or => or(*self, other),
            BinaryOp::Gt
            | BinaryOp::Lt
            | BinaryOp::Ge
            | BinaryOp::Le => cmp(*self, op, other),
        }
    }

    fn bin_op_assign(
        &mut self,
        op: BinaryOp,
        other: Value,
        heap: &mut Heap,
    ) -> Result<(), String> {
        match op {
            BinaryOp::Add => {
                if self.get_tag() == TAG_STR {
                    let id = self.as_obj_id();
                    if other.get_tag() == TAG_STR {
                        let other_id = other.as_obj_id();
                        let other_s = if let ManagedObject::Str(s) = heap.get(other_id) {
                            s.as_str().to_string()
                        } else {
                            return Err(NOT_A_STRING.to_string());
                        };
                        let s_ptr = if let ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err(NOT_A_STRING.to_string());
                        };
                        s_ptr.push_str(&other_s);
                        Ok(())
                    } else {
                        let bs = self.to_string_lossy(heap);
                        let other_bs = other.to_string_lossy(heap);
                        let s_ptr = if let ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err(NOT_A_STRING.to_string());
                        };
                        // Actually we need to append other to self, not bs
                        let _ = bs; // unused
                        s_ptr.push_str(&other_bs);
                        Ok(())
                    }
                } else {
                    *self = self.bin_op(op, other)?;
                    Ok(())
                }
            }
            _ => {
                *self = self.bin_op(op, other)?;
                Ok(())
            }
        }
    }

    fn to_string_lossy(&self, heap: &Heap) -> String {
        if self.is_unit() { return "()".to_string(); }
        if self.is_bool() { return if self.as_bool() { "true" } else { "false" }.to_string(); }
        if self.is_int() { return i64_to_string_fast(self.as_i64()); }
        if self.is_f64() {
            let f = self.as_f64();
            return if f.fract() == 0.0 { format!("{}", f as i64) } else { f.to_string() };
        }
        if self.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = heap.get(self.as_obj_id()) {
                return s.as_str().to_string();
            }
        }
        format!("{:?}", self)
    }
}

// ============================================================================
// Helper functions for binary operations
// ============================================================================

/// 将 Value 转换为 f64（如果可能）
#[inline]
fn to_f64(v: Value) -> Result<f64, String> {
    if v.is_f64() { Ok(v.as_f64()) }
    else if v.is_int() { Ok(v.as_i64() as f64) }
    else { Err(format!("[E0003] Expected numeric type, got {}", v.type_name())) }
}

fn coerce_f64(a: Value, b: Value) -> Result<(f64, f64), String> {
    Ok((to_f64(a)?, to_f64(b)?))
}

fn add(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(a.as_i64().saturating_add(b.as_i64())));
    }
    if a.is_f64() || b.is_f64() {
        return Ok(Value::from_f64(to_f64(a)? + to_f64(b)?));
    }
    Err("Operand mismatch for add (String concat requires heap access)".into())
}

fn sub(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(a.as_i64().saturating_sub(b.as_i64())));
    }
    let (x, y) = coerce_f64(a, b)?;
    Ok(Value::from_f64(x - y))
}

fn mul(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(a.as_i64().saturating_mul(b.as_i64())));
    }
    let (x, y) = coerce_f64(a, b)?;
    Ok(Value::from_f64(x * y))
}

fn div(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv == 0 { return Err("Division by zero".to_string()); }
        return a.as_i64().checked_div(bv).map(Value::from_i64).ok_or_else(|| "Integer division overflow".to_string());
    }
    let (x, y) = coerce_f64(a, b)?;
    if y == 0.0 { return Err("Division by zero".to_string()); }
    Ok(Value::from_f64(x / y))
}

fn rem(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv == 0 { return Err("Division by zero".to_string()); }
        return Ok(Value::from_i64(a.as_i64() % bv));
    }
    let (x, y) = coerce_f64(a, b)?;
    if y == 0.0 { return Err("Division by zero".to_string()); }
    Ok(Value::from_f64(x % y))
}

fn and(a: Value, b: Value) -> Result<Value, String> {
    if a.is_bool() && b.is_bool() { Ok(Value::from_bool(a.as_bool() && b.as_bool())) }
    else { Err("Logical AND requires both operands to be of type ?".to_string()) }
}

fn or(a: Value, b: Value) -> Result<Value, String> {
    if a.is_bool() && b.is_bool() { Ok(Value::from_bool(a.as_bool() || b.as_bool())) }
    else { Err("Logical OR requires both operands to be of type ?".to_string()) }
}

fn cmp(a: Value, op: BinaryOp, b: Value) -> Result<Value, String> {
    let res = if a.is_int() && b.is_int() {
        let (av, bv) = (a.as_i64(), b.as_i64());
        match op {
            BinaryOp::Gt => av > bv, BinaryOp::Lt => av < bv,
            BinaryOp::Ge => av >= bv, BinaryOp::Le => av <= bv,
            _ => unreachable!(),
        }
    } else {
        let (x, y) = coerce_f64(a, b)?;
        match op {
            BinaryOp::Gt => x > y, BinaryOp::Lt => x < y,
            BinaryOp::Ge => x >= y, BinaryOp::Le => x <= y,
            _ => unreachable!(),
        }
    };
    Ok(Value::from_bool(res))
}

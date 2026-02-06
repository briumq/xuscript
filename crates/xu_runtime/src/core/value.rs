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

/// Compact dict key representation using heap references.
/// String keys store only the ObjectId reference (no string copy).
/// This dramatically improves dict insertion performance.
#[derive(Clone, Copy)]
pub enum DictKey {
    /// String key - stores hash and ObjectId reference to heap string
    /// No string content is copied, only the reference is stored
    StrRef { hash: u64, obj_id: usize },
    /// Integer key
    Int(i64),
}

impl fmt::Debug for DictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictKey::StrRef { hash, obj_id } => {
                f.debug_struct("StrRef").field("hash", hash).field("obj_id", obj_id).finish()
            }
            DictKey::Int(i) => f.debug_tuple("Int").field(i).finish(),
        }
    }
}

impl DictKey {
    pub fn is_str(&self) -> bool {
        matches!(self, DictKey::StrRef { .. })
    }

    pub fn is_int(&self) -> bool {
        matches!(self, DictKey::Int(_))
    }

    /// Create a string key from ObjectId (no string copy!)
    #[inline]
    pub fn from_str_obj(obj_id: ObjectId, hash: u64) -> Self {
        DictKey::StrRef { hash, obj_id: obj_id.0 }
    }

    /// Create a string key by allocating a new string on the heap
    #[inline]
    pub fn from_str_alloc(s: &str, heap: &mut Heap) -> Self {
        let hash = Self::hash_str(s);
        let obj_id = heap.alloc(ManagedObject::Str(Text::from_str(s)));
        DictKey::StrRef { hash, obj_id: obj_id.0 }
    }

    /// Create a string key from Text by allocating on heap
    #[inline]
    pub fn from_text_alloc(t: Text, heap: &mut Heap) -> Self {
        let hash = Self::hash_str(t.as_str());
        let obj_id = heap.alloc(ManagedObject::Str(t));
        DictKey::StrRef { hash, obj_id: obj_id.0 }
    }

    /// Compute hash for a string (used for fast equality comparison)
    #[inline]
    pub fn hash_str(s: &str) -> u64 {
        use std::hash::Hasher;
        let mut hasher = ahash::AHasher::default();
        hasher.write(s.as_bytes());
        hasher.finish()
    }

    /// Get the string content by looking up in heap
    /// Returns None if not a string key
    #[inline]
    pub fn as_str<'a>(&self, heap: &'a Heap) -> Option<&'a str> {
        match self {
            DictKey::StrRef { obj_id, .. } => {
                if let ManagedObject::Str(s) = heap.get(ObjectId(*obj_id)) {
                    Some(s.as_str())
                } else {
                    None
                }
            }
            DictKey::Int(_) => None,
        }
    }

    /// Get the ObjectId for string keys
    #[inline]
    pub fn str_obj_id(&self) -> Option<ObjectId> {
        match self {
            DictKey::StrRef { obj_id, .. } => Some(ObjectId(*obj_id)),
            DictKey::Int(_) => None,
        }
    }

    /// Get the hash value
    #[inline]
    pub fn get_hash(&self) -> u64 {
        match self {
            DictKey::StrRef { hash, .. } => *hash,
            DictKey::Int(i) => {
                use std::hash::Hasher;
                let mut hasher = ahash::AHasher::default();
                hasher.write_i64(*i);
                hasher.finish()
            }
        }
    }

    /// Get integer value (panics if not Int)
    #[inline]
    pub fn as_int(&self) -> i64 {
        match self {
            DictKey::Int(i) => *i,
            _ => panic!("DictKey::as_int called on non-Int"),
        }
    }

    /// Compare two DictKeys for equality, using heap for string comparison
    #[inline]
    pub fn eq_with_heap(&self, other: &Self, heap: &Heap) -> bool {
        match (self, other) {
            (DictKey::StrRef { hash: h1, obj_id: id1 }, DictKey::StrRef { hash: h2, obj_id: id2 }) => {
                // Fast path: same object
                if id1 == id2 {
                    return true;
                }
                // Fast path: different hash means different string
                if h1 != h2 {
                    return false;
                }
                // Slow path: compare string content (hash collision)
                if let (Some(s1), Some(s2)) = (self.as_str(heap), other.as_str(heap)) {
                    s1 == s2
                } else {
                    false
                }
            }
            (DictKey::Int(a), DictKey::Int(b)) => a == b,
            _ => false,
        }
    }

    /// Compare DictKey with a string slice
    #[inline]
    pub fn eq_str(&self, s: &str, s_hash: u64, heap: &Heap) -> bool {
        match self {
            DictKey::StrRef { hash, obj_id } => {
                if *hash != s_hash {
                    return false;
                }
                if let ManagedObject::Str(text) = heap.get(ObjectId(*obj_id)) {
                    text.as_str() == s
                } else {
                    false
                }
            }
            DictKey::Int(_) => false,
        }
    }

    /// Compare DictKey with an integer
    #[inline]
    pub fn eq_int(&self, i: i64) -> bool {
        matches!(self, DictKey::Int(v) if *v == i)
    }
}

/// PartialEq implementation - compares by hash for string keys
/// This assumes hash collisions are rare (ahash is high quality)
impl PartialEq for DictKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DictKey::StrRef { hash: h1, obj_id: id1 }, DictKey::StrRef { hash: h2, obj_id: id2 }) => {
                // Same obj_id means same string
                if id1 == id2 {
                    return true;
                }
                // Same hash means same string (assuming no collision)
                h1 == h2
            }
            (DictKey::Int(a), DictKey::Int(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for DictKey {}

impl Hash for DictKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            DictKey::StrRef { hash, .. } => {
                state.write_u8(0);
                state.write_u64(*hash);
            }
            DictKey::Int(i) => {
                state.write_u8(1);
                i.hash(state);
            }
        }
    }
}

impl fmt::Display for DictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictKey::StrRef { obj_id, .. } => write!(f, "<str@{}>", obj_id),
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
            map.insert(k.clone(), *v);
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
            map.insert(k.clone(), *v);
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

//! Runtime values.
//!
//! Defines the runtime value representation, including primitives and heap-managed objects.

use super::gc::{Heap, ObjectId};
use super::text::Text;
use ahash::RandomState;
use hashbrown::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use xu_ir::{Bytecode, FuncDef};

/// Compact dict key representation.
/// Str variant uses pre-computed hash + Rc<String> for memory efficiency.
/// This reduces DictKey from 24 bytes (with Text) to 16 bytes.
#[derive(Clone, Debug)]
pub enum DictKey {
    Str { hash: u64, data: Rc<String> },
    Int(i64),
}

/// Format an `i64` into a fixed-size buffer and return the written slice.
pub(crate) fn write_i64_to_buf(i: i64, buf: &mut [u8; 32]) -> &[u8] {
    const LUT: &[u8; 200] = b"0001020304050607080910111213141516171819\
2021222324252627282930313233343536373839\
4041424344454647484950515253545556575859\
6061626364656667686970717273747576777879\
8081828384858687888990919293949596979899";

    let mut end = buf.len();
    if i == 0 {
        end -= 1;
        buf[end] = b'0';
        return &buf[end..];
    }
    let neg = i < 0;
    let mut n = if neg {
        i.wrapping_neg() as u64
    } else {
        i as u64
    };

    while n >= 100 {
        let rem = (n % 100) as usize;
        n /= 100;
        end -= 2;
        let idx = rem * 2;
        buf[end] = LUT[idx];
        buf[end + 1] = LUT[idx + 1];
    }
    if n < 10 {
        end -= 1;
        buf[end] = b'0' + n as u8;
    } else {
        let rem = n as usize;
        end -= 2;
        let idx = rem * 2;
        buf[end] = LUT[idx];
        buf[end + 1] = LUT[idx + 1];
    }
    if neg {
        end -= 1;
        buf[end] = b'-';
    }
    &buf[end..]
}

pub(crate) fn i64_to_string_fast(i: i64) -> String {
    let mut buf = [0u8; 32];
    let digits = write_i64_to_buf(i, &mut buf);
    let s = unsafe { std::str::from_utf8_unchecked(digits) };
    let mut out = String::with_capacity(s.len());
    out.push_str(s);
    out
}

pub(crate) fn i64_to_text_fast(i: i64) -> Text {
    let mut buf = [0u8; 32];
    let digits = write_i64_to_buf(i, &mut buf);
    let s = unsafe { std::str::from_utf8_unchecked(digits) };
    Text::from_str(s)
}

impl PartialEq for DictKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DictKey::Str { hash: h1, data: d1 }, DictKey::Str { hash: h2, data: d2 }) => {
                // Fast path: compare hash first
                if h1 != h2 {
                    return false;
                }
                // Fast path: same Rc pointer means same string
                if Rc::ptr_eq(d1, d2) {
                    return true;
                }
                // Slow path: compare string content (hash collision)
                d1.as_str() == d2.as_str()
            }
            (DictKey::Int(a), DictKey::Int(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for DictKey {}

impl DictKey {
    pub fn is_str(&self) -> bool {
        matches!(self, DictKey::Str { .. })
    }

    /// Create a new string key with pre-computed hash
    pub fn from_str(s: &str) -> Self {
        let hash = Self::hash_str(s);
        DictKey::Str { hash, data: Rc::new(s.to_string()) }
    }

    /// Create a new string key from Rc<String> with pre-computed hash
    pub fn from_rc(data: Rc<String>) -> Self {
        let hash = Self::hash_str(&data);
        DictKey::Str { hash, data }
    }

    /// Create a new string key from Text
    pub fn from_text(t: &Text) -> Self {
        Self::from_str(t.as_str())
    }

    /// Compute hash for a string (used for fast equality comparison)
    #[inline]
    pub fn hash_str(s: &str) -> u64 {
        use std::hash::Hasher;
        // Use a simple fast hash for equality comparison
        let mut hasher = ahash::AHasher::default();
        hasher.write(s.as_bytes());
        hasher.finish()
    }

    /// Get the string content (panics if not a string key)
    pub fn as_str(&self) -> &str {
        match self {
            DictKey::Str { data, .. } => data.as_str(),
            DictKey::Int(_) => panic!("DictKey::as_str called on Int"),
        }
    }
}

impl Hash for DictKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            DictKey::Str { data, .. } => {
                state.write_u8(0);
                // Hash the actual string content for HashMap compatibility
                data.as_bytes().hash(state);
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
            DictKey::Str { data, .. } => write!(f, "{}", data),
            DictKey::Int(i) => write!(f, "{}", i),
        }
    }
}

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

// NaN-Boxing constants
pub const QNAN: u64 = 0x7ff8000000000000;
pub const TAG_BASE: u64 = 0xfff0000000000000;
pub const TAG_MASK: u64 = 0x000f000000000000;
pub const PAYLOAD_MASK: u64 = 0x0000ffffffffffff;

pub const TAG_INT: u64 = 0x0001;
pub const TAG_BOOL: u64 = 0x0002;
pub const TAG_VOID: u64 = 0x0003;

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
        Self::VOID
    }
}

impl Value {
    pub const VOID: Value = Value(TAG_BASE | (TAG_VOID << 48));

    pub fn none() -> Self {
        Self::VOID
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
    pub fn is_void(&self) -> bool {
        !self.is_f64() && self.get_tag() == TAG_VOID
    }
    #[inline(always)]
    pub fn is_obj(&self) -> bool {
        !self.is_f64() && self.get_tag() > TAG_VOID
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

    pub fn mark_into(&self, heap: &mut Heap, pending: &mut Vec<ObjectId>) {
        if self.is_obj() {
            let id = self.as_obj_id();
            if !heap.is_marked(id) {
                pending.push(id);
            }
        }
    }
}

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

#[derive(Clone)]
pub struct FileHandle {
    pub path: String,
    pub open: bool,
    pub content: String,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_f64() {
            write!(f, "Float({})", self.as_f64())
        } else if self.is_int() {
            write!(f, "Int({})", self.as_i64())
        } else if self.is_bool() {
            write!(f, "Bool({})", self.as_bool())
        } else if self.is_void() {
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

impl Value {
    pub fn type_name(&self) -> &'static str {
        if self.is_f64() {
            "float"
        } else if self.is_int() {
            "int"
        } else if self.is_bool() {
            "bool"
        } else if self.is_void() {
            "void"
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

    pub fn bin_op(&self, op: xu_ir::BinaryOp, other: Value) -> Result<Value, String> {
        match op {
            xu_ir::BinaryOp::Add => self.add(other),
            xu_ir::BinaryOp::Sub => self.sub(other),
            xu_ir::BinaryOp::Mul => self.mul(other),
            xu_ir::BinaryOp::Div => self.div(other),
            xu_ir::BinaryOp::Mod => self.rem(other),
            xu_ir::BinaryOp::Eq => Ok(Value::from_bool(self == &other)),
            xu_ir::BinaryOp::Ne => Ok(Value::from_bool(self != &other)),
            xu_ir::BinaryOp::And => self.and(other),
            xu_ir::BinaryOp::Or => self.or(other),
            xu_ir::BinaryOp::Gt
            | xu_ir::BinaryOp::Lt
            | xu_ir::BinaryOp::Ge
            | xu_ir::BinaryOp::Le => self.cmp(op, other),
        }
    }

    pub fn bin_op_assign(
        &mut self,
        op: xu_ir::BinaryOp,
        other: Value,
        heap: &mut Heap,
    ) -> Result<(), String> {
        match op {
            xu_ir::BinaryOp::Add => {
                if self.get_tag() == TAG_STR {
                    let id = self.as_obj_id();
                    if other.get_tag() == TAG_STR {
                        let other_id = other.as_obj_id();
                        let other_s = if let super::gc::ManagedObject::Str(s) = heap.get(other_id) {
                            s.as_str().to_string()
                        } else {
                            return Err("Not a string".to_string());
                        };
                        let s_ptr = if let super::gc::ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err("Not a string".to_string());
                        };
                        s_ptr.push_str(&other_s);
                        Ok(())
                    } else {
                        let bs = other.to_string_lossy(heap);
                        let s_ptr = if let super::gc::ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err("Not a string".to_string());
                        };
                        s_ptr.push_str(&bs);
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

    fn add(&self, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            return Ok(Value::from_i64(
                self.as_i64().saturating_add(other.as_i64()),
            ));
        }
        if self.is_f64() || other.is_f64() {
            let x = if self.is_f64() {
                self.as_f64()
            } else if self.is_int() {
                self.as_i64() as f64
            } else {
                return Err("Invalid operand".into());
            };
            let y = if other.is_f64() {
                other.as_f64()
            } else if other.is_int() {
                other.as_i64() as f64
            } else {
                return Err("Invalid operand".into());
            };
            return Ok(Value::from_f64(x + y));
        }

        Err("Operand mismatch for add (String concat requires heap access)".into())
    }

    fn sub(&self, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            return Ok(Value::from_i64(
                self.as_i64().saturating_sub(other.as_i64()),
            ));
        }
        let (x, y) = self.coerce_f64(&other)?;
        Ok(Value::from_f64(x - y))
    }

    fn mul(&self, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            return Ok(Value::from_i64(
                self.as_i64().saturating_mul(other.as_i64()),
            ));
        }
        let (x, y) = self.coerce_f64(&other)?;
        Ok(Value::from_f64(x * y))
    }

    fn div(&self, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            let b = other.as_i64();
            if b == 0 {
                return Err("Division by zero".to_string());
            }
            return self
                .as_i64()
                .checked_div(b)
                .map(Value::from_i64)
                .ok_or_else(|| "Integer division overflow".to_string());
        }
        let (x, y) = self.coerce_f64(&other)?;
        if y == 0.0 {
            return Err("Division by zero".to_string());
        }
        Ok(Value::from_f64(x / y))
    }

    fn rem(&self, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            let b = other.as_i64();
            if b == 0 {
                return Err("Division by zero".to_string());
            }
            return Ok(Value::from_i64(self.as_i64() % b));
        }
        let (x, y) = self.coerce_f64(&other)?;
        if y == 0.0 {
            return Err("Division by zero".to_string());
        }
        Ok(Value::from_f64(x % y))
    }

    fn and(&self, other: Value) -> Result<Value, String> {
        if self.is_bool() && other.is_bool() {
            return Ok(Value::from_bool(self.as_bool() && other.as_bool()));
        }
        Err("Logical AND requires both operands to be of type ?".to_string())
    }

    fn or(&self, other: Value) -> Result<Value, String> {
        if self.is_bool() && other.is_bool() {
            return Ok(Value::from_bool(self.as_bool() || other.as_bool()));
        }
        Err("Logical OR requires both operands to be of type ?".to_string())
    }

    fn cmp(&self, op: xu_ir::BinaryOp, other: Value) -> Result<Value, String> {
        if self.is_int() && other.is_int() {
            let a = self.as_i64();
            let b = other.as_i64();
            let res = match op {
                xu_ir::BinaryOp::Gt => a > b,
                xu_ir::BinaryOp::Lt => a < b,
                xu_ir::BinaryOp::Ge => a >= b,
                xu_ir::BinaryOp::Le => a <= b,
                _ => unreachable!(),
            };
            return Ok(Value::from_bool(res));
        }
        let (x, y) = self.coerce_f64(&other)?;
        let res = match op {
            xu_ir::BinaryOp::Gt => x > y,
            xu_ir::BinaryOp::Lt => x < y,
            xu_ir::BinaryOp::Ge => x >= y,
            xu_ir::BinaryOp::Le => x <= y,
            _ => unreachable!(),
        };
        Ok(Value::from_bool(res))
    }

    fn coerce_f64(&self, other: &Value) -> Result<(f64, f64), String> {
        let a = if self.is_f64() {
            self.as_f64()
        } else if self.is_int() {
            self.as_i64() as f64
        } else {
            return Err(format!(
                "[E0003] Expected numeric type, got {}",
                self.type_name()
            ));
        };
        let b = if other.is_f64() {
            other.as_f64()
        } else if other.is_int() {
            other.as_i64() as f64
        } else {
            return Err(format!(
                "[E0003] Expected numeric type, got {}",
                other.type_name()
            ));
        };
        Ok((a, b))
    }

    pub fn to_string_lossy(&self, heap: &Heap) -> String {
        if self.is_void() {
            "()".to_string()
        } else if self.is_bool() {
            if self.as_bool() {
                "true".to_string()
            } else {
                "false".to_string()
            }
        } else if self.is_int() {
            i64_to_string_fast(self.as_i64())
        } else if self.is_f64() {
            let f = self.as_f64();
            if f.fract() == 0.0 {
                format!("{}", f as i64)
            } else {
                f.to_string()
            }
        } else if self.get_tag() == TAG_STR {
            if let super::gc::ManagedObject::Str(s) = heap.get(self.as_obj_id()) {
                s.as_str().to_string()
            } else {
                format!("{:?}", self)
            }
        } else {
            format!("{:?}", self)
        }
    }
}

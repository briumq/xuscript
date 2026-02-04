//!
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
///
pub enum Type {
    Any,
    Unit,
    Bool,
    Int,
    Float,
    Text,
    Function,
    Range,
    List(TypeId),
    Dict(TypeId, TypeId),
    Struct(String),
    Enum(String),
}

pub type TypeId = u32;

///
pub struct TypeInterner {
    map: HashMap<Type, TypeId>,
    rev: Vec<Type>,
}

impl TypeInterner {
    pub fn new() -> Self {
        let mut this = Self {
            map: HashMap::new(),
            rev: Vec::new(),
        };
        // Builtins
        for ty in [
            Type::Any,
            Type::Unit,
            Type::Bool,
            Type::Int,
            Type::Float,
            Type::Text,
            Type::Function,
        ] {
            this.intern(ty);
        }
        this
    }

    pub fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(id) = self.map.get(&ty).cloned() {
            return id;
        }
        let id = self.rev.len() as TypeId;
        self.rev.push(ty.clone());
        self.map.insert(ty, id);
        id
    }

    pub fn get(&self, id: TypeId) -> &Type {
        &self.rev[id as usize]
    }

    pub fn name(&self, id: TypeId) -> String {
        match self.get(id) {
            Type::Any => "any".to_string(),
            Type::Unit => "unit".to_string(),
            Type::Bool => "?".to_string(),
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Text => "text".to_string(),
            Type::Function => "func".to_string(),
            Type::Range => "range".to_string(),
            Type::List(elem) => format!("list[{}]", self.name(*elem)),
            Type::Dict(k, v) => format!("dict[{}, {}]", self.name(*k), self.name(*v)),
            Type::Struct(s) => s.clone(),
            Type::Enum(s) => s.clone(),
        }
    }

    pub fn list(&mut self, elem: TypeId) -> TypeId {
        self.intern(Type::List(elem))
    }
    pub fn dict(&mut self, key: TypeId, val: TypeId) -> TypeId {
        self.intern(Type::Dict(key, val))
    }

    pub fn builtin_by_name(&mut self, name: &str) -> Option<TypeId> {
        match name {
            "any" => Some(self.intern(Type::Any)),
            "unit" | "()" => Some(self.intern(Type::Unit)),
            "?" | "bool" => Some(self.intern(Type::Bool)),
            "int" => Some(self.intern(Type::Int)),
            "float" => Some(self.intern(Type::Float)),
            "text" | "str" | "string" => Some(self.intern(Type::Text)),
            "func" => Some(self.intern(Type::Function)),
            "range" => Some(self.intern(Type::Range)),
            _ => None,
        }
    }

    ///
    pub fn parse_type_str(&mut self, s: &str) -> TypeId {
        let s = s.trim();
        if s == "list" {
            let any = self.intern(Type::Any);
            return self.list(any);
        }
        if s == "dict" {
            let k = self.intern(Type::Text);
            let v = self.intern(Type::Any);
            return self.dict(k, v);
        }
        if let Some(id) = self.builtin_by_name(s) {
            return id;
        }
        if let Some(idx) = s.find('[') {
            let base = &s[..idx];
            let inner = &s[idx + 1..s.len() - 1];
            if base == "list" {
                let elem = self.parse_type_str(inner);
                return self.list(elem);
            }
            if base == "dict" {
                let (k, v) = split_type_params(inner);
                let kid = self.parse_type_str(k);
                let vid = self.parse_type_str(v);
                return self.dict(kid, vid);
            }
        }
        // Assume user-defined type
        self.intern(Type::Struct(s.to_string()))
    }
}

fn split_type_params(s: &str) -> (&str, &str) {
    let mut depth = 0usize;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b',' if depth == 0 => {
                let left = &s[..i].trim();
                let right = &s[i + 1..].trim();
                return (left, right);
            }
            _ => {}
        }
    }
    (s, "")
}

//!
//!
//!
//!

use crate::{AssignOp, EnumDef, FuncDef, Pattern, StructDef};

#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeFunction {
    pub def: FuncDef,
    pub bytecode: Box<Bytecode>,
    pub locals_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Str(String),
    Int(i64),
    Float(f64),
    Struct(StructDef),
    Enum(EnumDef),
    Func(BytecodeFunction),
    Names(Vec<String>),
    Pattern(Pattern),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    ConstInt(i64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstNull,
    Const(u32), // Index into constant pool
    Pop,
    Dup,        // Duplicate top of stack
    Add,
    AddAssignName(u32), // Index into constant pool (String)
    AddAssignLocal(usize),
    Sub,
    Mul,
    Div,
    Mod,
    StrAppend,
    StrAppendNull,
    StrAppendBool,
    StrAppendInt,
    StrAppendFloat,
    StrAppendStr,
    Eq,
    Ne,
    And,
    Or,
    Gt,
    Lt,
    Ge,
    Le,
    Not,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    LoadName(u32), // Index into constant pool (String)
    LoadLocal(usize),
    StoreName(u32), // Index into constant pool (String)
    StoreLocal(usize),
    Use(u32, u32), // (path_str_idx, alias_str_idx)
    AssertType(u32),            // Index into constant pool (String)
    DefineStruct(u32),          // Index into constant pool (StructDef)
    DefineEnum(u32),            // Index into constant pool (EnumDef)
    StructInit(u32, u32),       // Index to String, Index to Names (Vec<String>)
    EnumCtor(u32, u32),         // Index to String (Enum name), Index to String (Ctor name)
    EnumCtorN(u32, u32, usize), // Index to String (Enum name), Index to String (Ctor name), args_count
    EnumCtorMod(u32, u32),      // Module enum: stack has module, (type_idx, variant_idx)
    EnumCtorModN(u32, u32, usize), // Module enum with args: stack has module + args, (type_idx, variant_idx, args_count)
    MakeFunction(u32),          // Index into constant pool (BytecodeFunction)
    Call(usize),
    CallMethod(u32, u64, usize, Option<usize>), // Index to String (Method name), hash, args_count, slot
    Inc,
    IncLocal(usize),
    MakeRange(bool),
    GetMember(u32, Option<usize>), // Index to String (Member name), slot
    GetIndex(Option<usize>),
    AssignMember(u32, AssignOp), // Index to String (Member name), op
    AssignIndex(AssignOp),
    Break(usize),
    Continue(usize),
    Return,
    Throw,
    TryPush(usize, usize, usize, Option<usize>), // catch_ip, finally_ip, end_ip, slot
    TryPop,
    RunPending,
    SetThrown,
    // Builder specialized ops
    BuilderNewCap(usize),
    BuilderAppend,
    BuilderFinalize,
    DictGetStrConst(u32, u64, Option<usize>), // Index to String, hash, slot
    DictGetIntConst(i64, Option<usize>),
    ForEachInit(u32, Option<usize>, usize), // Index to String, slot, local_idx
    ForEachNext(u32, Option<usize>, usize, usize), // Index to String, slot, local_idx, jump_addr
    IterPop,
    EnvPush,
    EnvPop,
    ListNew(usize),
    TupleNew(usize),
    DictNew(usize),
    DictInsert,
    DictInsertStrConst(u32, u64, Option<usize>), // Index to String (key), hash, slot
    DictMerge,
    ListAppend(usize),
    Print,
    Halt,
    // Match pattern ops
    MatchPattern(u32),          // Index to Pattern constant, pushes bool (match result)
    MatchBindings(u32),         // Index to Pattern constant, pushes bindings count then values
    LocalsPush,                 // Push a new locals frame
    LocalsPop,                  // Pop locals frame
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytecode {
    pub ops: Vec<Op>,
    pub constants: Vec<Constant>,
}

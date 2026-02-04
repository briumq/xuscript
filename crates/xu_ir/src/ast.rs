use xu_syntax::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub stmts: Box<[Stmt]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    StructDef(Box<StructDef>),
    EnumDef(Box<EnumDef>),
    FuncDef(Box<FuncDef>),
    DoesBlock(Box<DoesBlock>),
    Use(Box<UseStmt>),
    If(Box<IfStmt>),
    While(Box<WhileStmt>),
    ForEach(Box<ForEachStmt>),
    Match(Box<MatchStmt>),
    Block(Box<[Stmt]>),
    Return(Option<Expr>),
    Break,
    Continue,
    Assign(Box<AssignStmt>),
    Expr(Expr),
    Error(Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Inner,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDef {
    pub vis: Visibility,
    pub name: String,
    pub fields: Box<[StructField]>,
    pub static_fields: Box<[StaticField]>,
    pub methods: Box<[FuncDef]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDef {
    pub vis: Visibility,
    pub name: String,
    pub variants: Box<[String]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: TypeRef,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticField {
    pub name: String,
    pub ty: TypeRef,
    pub default: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncDef {
    pub vis: Visibility,
    pub name: String,
    pub params: Box<[Param]>,
    pub return_ty: Option<TypeRef>,
    pub body: Box<[Stmt]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DoesBlock {
    pub vis: Visibility,
    pub target: String,
    pub funcs: Box<[FuncDef]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UseStmt {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub name: String,
    pub params: Box<[TypeRef]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    pub branches: Box<[(Expr, Box<[Stmt]>)]>,
    pub else_branch: Option<Box<[Stmt]>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Box<[Stmt]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForEachStmt {
    pub iter: Expr,
    pub var: String,
    pub body: Box<[Stmt]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Wildcard,
    Bind(String),
    Tuple(Box<[Pattern]>),
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    EnumVariant {
        ty: String,
        variant: String,
        args: Box<[Pattern]>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchStmt {
    pub expr: Expr,
    pub arms: Box<[(Pattern, Box<[Stmt]>)]>,
    pub else_branch: Option<Box<[Stmt]>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssignStmt {
    pub vis: Visibility,
    pub target: Expr,
    pub op: AssignOp,
    pub value: Expr,
    pub ty: Option<TypeRef>,
    pub slot: Option<(u32, u32)>, // (depth_diff, index)
    pub decl: Option<DeclKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Let,
    Var,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Ident(String, std::cell::Cell<Option<(u32, u32)>>),
    Int(i64),
    Float(f64),
    Str(String),
    InterpolatedString(Box<[Expr]>),
    Bool(bool),
    List(Box<[Expr]>),
    Tuple(Box<[Expr]>),
    Range(Box<RangeExpr>),
    IfExpr(Box<IfExpr>),
    Match(Box<MatchExpr>),
    FuncLit(Box<FuncDef>),
    Dict(Box<[(String, Expr)]>),
    StructInit(Box<StructInitExpr>),
    EnumCtor {
        module: Option<Box<Expr>>,
        ty: String,
        variant: String,
        args: Box<[Expr]>,
    },
    Member(Box<MemberExpr>),
    Index(Box<IndexExpr>),
    Call(Box<CallExpr>),
    MethodCall(Box<MethodCallExpr>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Group(Box<Expr>),
    Error(Span),
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfExpr {
    pub cond: Box<Expr>,
    pub then_expr: Box<Expr>,
    pub else_expr: Box<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchExpr {
    pub expr: Box<Expr>,
    pub arms: Box<[(Pattern, Expr)]>,
    pub else_expr: Option<Box<Expr>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RangeExpr {
    pub start: Box<Expr>,
    pub end: Box<Expr>,
    pub inclusive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StructInitItem {
    Spread(Expr),
    Field(String, Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructInitExpr {
    pub module: Option<Box<Expr>>,
    pub ty: String,
    pub items: Box<[StructInitItem]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemberExpr {
    pub object: Box<Expr>,
    pub field: String,
    pub ic_slot: std::cell::Cell<Option<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexExpr {
    pub object: Box<Expr>,
    pub index: Box<Expr>,
    pub ic_slot: std::cell::Cell<Option<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Box<[Expr]>,
}

/// Receiver type hint for method calls, used by the compiler to generate
/// specialized opcodes for built-in types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverType {
    List,
    Dict,
    Struct,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MethodCallExpr {
    pub receiver: Box<Expr>,
    pub method: String,
    pub args: Box<[Expr]>,
    pub ic_slot: std::cell::Cell<Option<usize>>,
    pub receiver_ty: std::cell::Cell<Option<ReceiverType>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    And,
    Or,
}

impl Expr {
    /// Returns true if this expression can be assigned to.
    pub fn is_assignable(&self) -> bool {
        matches!(self, Expr::Ident(_, _) | Expr::Member(_) | Expr::Index(_))
    }
}

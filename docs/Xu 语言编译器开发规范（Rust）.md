# Xu 语言编译器开发规范（Rust）

---

## 一、项目结构

text

```
xu-lang/
├── Cargo.toml
├── src/
│   ├── main.rs              # 入口
│   ├── lib.rs               # 库导出
│   ├── lexer/               # 词法分析
│   │   ├── mod.rs
│   │   ├── token.rs         # Token 定义
│   │   └── scanner.rs       # 扫描器
│   ├── parser/              # 语法分析
│   │   ├── mod.rs
│   │   ├── ast.rs           # AST 定义
│   │   └── parser.rs        # 解析器
│   ├── semantic/            # 语义分析
│   │   ├── mod.rs
│   │   ├── resolver.rs      # 名称解析
│   │   └── type_checker.rs  # 类型检查
│   ├── interpreter/         # 解释执行（可选）
│   │   ├── mod.rs
│   │   ├── value.rs         # 运行时值
│   │   └── eval.rs          # 求值器
│   ├── codegen/             # 代码生成（可选）
│   │   └── mod.rs
│   ├── stdlib/              # 标准库
│   │   └── mod.rs
│   ├── error.rs             # 错误处理
│   └── span.rs              # 源码位置
├── tests/                   # 集成测试
│   ├── lexer_tests.rs
│   ├── parser_tests.rs
│   └── eval_tests.rs
└── examples/                # Xu 示例代码
    └── hello.xu
```

---

## 二、注释规范

### 2.1 文件头注释

每个 `.rs` 文件必须以文件头注释开始：

Rust

```
//! Token 定义模块
//!
//! 定义 Xu 语言的所有 Token 类型，包括：
//! - 关键字（25 个）
//! - 运算符与分隔符
//! - 字面量（整数、浮点、字符串、布尔）
//! - 标识符
//!
//! # 示例
//!
//! ```
//! use xu_lang::lexer::Token;
//!
//! let token = Token::Keyword(Keyword::If);
//! ```
```

### 2.2 模块注释

`mod.rs` 文件说明模块职责：

Rust

```
//! 词法分析模块
//!
//! 负责将源代码转换为 Token 流。
//!
//! ## 主要组件
//!
//! - [`Token`] - Token 类型定义
//! - [`Scanner`] - 词法扫描器
//! - [`Span`] - 源码位置信息
//!
//! ## 使用流程
//!
//! ```text
//! 源代码 (String) -> Scanner -> Vec<Token>
//! ```

pub mod token;
pub mod scanner;

pub use token::*;
pub use scanner::*;
```

### 2.3 结构体/枚举注释

Rust

```
/// Xu 语言的 Token 类型
///
/// 每个 Token 包含类型信息和源码位置。
///
/// # 分类
///
/// - 关键字：`if`, `else`, `when` 等
/// - 运算符：`+`, `-`, `is`, `and` 等
/// - 字面量：整数、浮点、字符串
/// - 标识符：变量名、函数名
///
/// # 示例
///
/// ```
/// let token = Token {
///     kind: TokenKind::Keyword(Keyword::Let),
///     span: Span::new(0, 3),
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token 类型
    pub kind: TokenKind,
    /// 源码位置
    pub span: Span,
}

/// Token 类型枚举
///
/// 包含 Xu 语言所有可能的 Token 类型。
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ========== 关键字 ==========
    
    /// `if` - 条件语句
    If,
    /// `else` - 否则分支
    Else,
    /// `while` - 循环语句
    While,
    /// `for` - 遍历语句
    For,
    /// `in` - 用于 for...in 和范围
    In,
    /// `break` - 跳出循环
    Break,
    /// `continue` - 继续下一轮
    Continue,
    /// `when` - 模式匹配/条件绑定
    When,
    
    /// `let` - 变量声明
    Let,
    /// `func` - 函数定义
    Func,
    /// `return` - 返回语句
    Return,
    /// `with` - 结构体/枚举定义
    With,
    /// `does` - 方法扩展块
    Does,
    
    /// `pub` - 公开可见性
    Pub,
    /// `static` - 静态方法
    Static,
    
    /// `self` - 当前实例引用
    SelfValue,
    /// `true` - 布尔真值
    True,
    /// `false` - 布尔假值
    False,
    
    /// `is` - 判等运算符（推荐）
    Is,
    /// `isnt` - 不等运算符（推荐）
    Isnt,
    /// `not` - 逻辑非
    Not,
    /// `and` - 逻辑与
    And,
    /// `or` - 逻辑或
    Or,
    
    /// `import` - 模块导入
    Import,
    /// `as` - 别名
    As,
    
    // ========== 运算符 ==========
    
    /// `+` - 加法
    Plus,
    /// `-` - 减法/负号
    Minus,
    /// `*` - 乘法
    Star,
    /// `/` - 除法
    Slash,
    /// `%` - 取模
    Percent,
    
    /// `=` - 赋值
    Eq,
    /// `+=` - 加等于
    PlusEq,
    /// `-=` - 减等于
    MinusEq,
    /// `*=` - 乘等于
    StarEq,
    /// `/=` - 除等于
    SlashEq,
    
    /// `==` - 等于（兼容）
    EqEq,
    /// `!=` - 不等于（兼容）
    BangEq,
    /// `<` - 小于
    Lt,
    /// `>` - 大于
    Gt,
    /// `<=` - 小于等于
    Le,
    /// `>=` - 大于等于
    Ge,
    
    // ========== 分隔符 ==========
    
    /// `.` - 属性访问
    Dot,
    /// `..` - 范围（不含终点）
    DotDot,
    /// `..=` - 范围（含终点）
    DotDotEq,
    /// `#` - 枚举变体访问
    Hash,
    /// `,` - 逗号分隔符
    Comma,
    /// `:` - 类型标注/键值对
    Colon,
    /// `->` - 返回类型/Lambda 箭头
    Arrow,
    /// `|` - Lambda 参数/枚举分隔
    Pipe,
    /// `_` - 通配符
    Underscore,
    /// `...` - 结构体展开
    Spread,
    
    /// `(` - 左括号
    LParen,
    /// `)` - 右括号
    RParen,
    /// `[` - 左方括号
    LBracket,
    /// `]` - 右方括号
    RBracket,
    /// `{` - 左花括号
    LBrace,
    /// `}` - 右花括号
    RBrace,
    
    // ========== 字面量 ==========
    
    /// 整数字面量
    Int(i64),
    /// 浮点数字面量
    Float(f64),
    /// 字符串字面量（已处理转义和插值）
    String(String),
    
    // ========== 其他 ==========
    
    /// 标识符
    Ident(String),
    /// 换行（用于自动分号插入）
    Newline,
    /// 文件结束
    Eof,
}
```

### 2.4 函数注释

Rust

```
impl Scanner {
    /// 创建新的词法扫描器
    ///
    /// # 参数
    ///
    /// * `source` - 源代码字符串
    ///
    /// # 返回值
    ///
    /// 返回初始化的 Scanner 实例
    ///
    /// # 示例
    ///
    /// ```
    /// let scanner = Scanner::new("let x = 1");
    /// ```
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            tokens: Vec::new(),
            start: 0,
            current: 0,
            line: 1,
            column: 1,
        }
    }

    /// 扫描所有 Token
    ///
    /// 将源代码转换为 Token 序列。遇到词法错误时返回 `Err`。
    ///
    /// # 返回值
    ///
    /// - `Ok(Vec<Token>)` - 成功时返回 Token 列表
    /// - `Err(LexError)` - 词法错误
    ///
    /// # 错误
    ///
    /// - 未闭合的字符串
    /// - 非法字符
    /// - 无效的数字格式
    ///
    /// # 示例
    ///
    /// ```
    /// let mut scanner = Scanner::new("let x = 42");
    /// let tokens = scanner.scan_tokens()?;
    /// assert_eq!(tokens.len(), 5); // let, x, =, 42, EOF
    /// ```
    pub fn scan_tokens(&mut self) -> Result<Vec<Token>, LexError> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token()?;
        }
        
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.current_span(),
        });
        
        Ok(self.tokens.clone())
    }

    /// 扫描单个 Token
    ///
    /// 从当前位置识别一个 Token，并推进扫描位置。
    ///
    /// # 处理逻辑
    ///
    /// 1. 跳过空白字符
    /// 2. 识别注释
    /// 3. 识别字符串（普通/多行/原始）
    /// 4. 识别数字（整数/浮点/十六进制/二进制）
    /// 5. 识别标识符和关键字
    /// 6. 识别运算符和分隔符
    fn scan_token(&mut self) -> Result<(), LexError> {
        let c = self.advance();
        
        match c {
            // 空白字符
            ' ' | '\t' | '\r' => {}
            
            // 换行（用于自动分号插入）
            '\n' => {
                self.line += 1;
                self.column = 1;
                self.add_token(TokenKind::Newline);
            }
            
            // 单字符 Token
            '(' => self.add_token(TokenKind::LParen),
            ')' => self.add_token(TokenKind::RParen),
            '[' => self.add_token(TokenKind::LBracket),
            ']' => self.add_token(TokenKind::RBracket),
            '{' => self.add_token(TokenKind::LBrace),
            '}' => self.add_token(TokenKind::RBrace),
            ',' => self.add_token(TokenKind::Comma),
            ':' => self.add_token(TokenKind::Colon),
            '#' => self.add_token(TokenKind::Hash),
            '%' => self.add_token(TokenKind::Percent),
            '_' => self.add_token(TokenKind::Underscore),
            
            // 可能是多字符的 Token
            '+' => {
                let kind = if self.match_char('=') {
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                };
                self.add_token(kind);
            }
            
            '-' => {
                let kind = if self.match_char('=') {
                    TokenKind::MinusEq
                } else if self.match_char('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                };
                self.add_token(kind);
            }
            
            '*' => {
                let kind = if self.match_char('=') {
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                };
                self.add_token(kind);
            }
            
            '/' => {
                if self.match_char('/') {
                    // 单行注释
                    self.skip_line_comment();
                } else if self.match_char('*') {
                    // 多行注释
                    self.skip_block_comment()?;
                } else if self.match_char('=') {
                    self.add_token(TokenKind::SlashEq);
                } else {
                    self.add_token(TokenKind::Slash);
                }
            }
            
            '=' => {
                let kind = if self.match_char('=') {
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                };
                self.add_token(kind);
            }
            
            '!' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::BangEq);
                } else {
                    return Err(self.error("意外的字符 '!'，是否想用 'not'？"));
                }
            }
            
            '<' => {
                let kind = if self.match_char('=') {
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                };
                self.add_token(kind);
            }
            
            '>' => {
                let kind = if self.match_char('=') {
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                };
                self.add_token(kind);
            }
            
            '.' => {
                if self.match_char('.') {
                    let kind = if self.match_char('=') {
                        TokenKind::DotDotEq
                    } else if self.match_char('.') {
                        TokenKind::Spread
                    } else {
                        TokenKind::DotDot
                    };
                    self.add_token(kind);
                } else {
                    self.add_token(TokenKind::Dot);
                }
            }
            
            '|' => self.add_token(TokenKind::Pipe),
            
            // 字符串
            '"' => self.scan_string()?,
            
            // 原始字符串
            'r' if self.check('"') => self.scan_raw_string()?,
            
            // 数字
            c if c.is_ascii_digit() => self.scan_number()?,
            
            // 标识符和关键字
            c if c.is_alphabetic() || c == '_' => self.scan_identifier(),
            
            _ => return Err(self.error(&format!("未知字符: '{}'", c))),
        }
        
        Ok(())
    }
}
```

### 2.5 复杂逻辑注释

Rust

```
/// 扫描字符串字面量
///
/// 支持三种字符串格式：
/// - 普通字符串: `"hello"`
/// - 多行字符串: `"""..."""`
/// - 带插值: `"Hello, {name}!"`
///
/// # 转义序列
///
/// | 序列 | 含义 |
/// |------|------|
/// | `\n` | 换行 |
/// | `\t` | 制表符 |
/// | `\r` | 回车 |
/// | `\\` | 反斜杠 |
/// | `\"` | 双引号 |
/// | `\{` | 左花括号（禁止插值） |
///
/// # 字符串插值
///
/// `{expr}` 会被解析为表达式，运行时求值并转为字符串。
///
/// ```xu
/// let name = "World"
/// println("Hello, {name}!")  // 输出: Hello, World!
/// ```
///
/// # 错误
///
/// - 未闭合的字符串
/// - 未闭合的插值表达式
/// - 无效的转义序列
fn scan_string(&mut self) -> Result<(), LexError> {
    // 检查是否是多行字符串 """
    let is_multiline = self.match_char('"') && self.match_char('"');
    
    let mut value = String::new();
    let mut has_interpolation = false;
    
    loop {
        if self.is_at_end() {
            return Err(self.error("未闭合的字符串"));
        }
        
        let c = self.peek();
        
        // 检查字符串结束
        if is_multiline {
            if c == '"' && self.peek_next() == Some('"') && self.peek_nth(2) == Some('"') {
                self.advance(); // 消耗三个引号
                self.advance();
                self.advance();
                break;
            }
        } else if c == '"' {
            self.advance();
            break;
        }
        
        // 处理换行
        if c == '\n' {
            if !is_multiline {
                return Err(self.error("字符串中不允许换行，使用 \"\"\" 多行字符串"));
            }
            self.line += 1;
            self.column = 1;
            value.push(self.advance());
            continue;
        }
        
        // 处理转义序列
        if c == '\\' {
            self.advance();
            let escaped = match self.advance() {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '"' => '"',
                '{' => '{',
                c => return Err(self.error(&format!("无效的转义序列: \\{}", c))),
            };
            value.push(escaped);
            continue;
        }
        
        // 处理插值
        if c == '{' {
            has_interpolation = true;
            // TODO: 解析插值表达式
            // 这里需要递归调用表达式解析器
        }
        
        value.push(self.advance());
    }
    
    self.add_token(TokenKind::String(value));
    Ok(())
}
```

### 2.6 TODO/FIXME 注释

Rust

```
// TODO: 支持 Unicode 标识符
// 当前只支持 ASCII 字母，需要扩展到 Unicode XID_Start/XID_Continue
fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

// FIXME: 负索引边界检查
// 当前 list[-1] 在空列表上会 panic，应该返回 Option
fn get_index(&self, index: i64) -> Value {
    // ...
}

// HACK: 临时解决方案
// 由于生命周期问题，这里克隆了整个 AST
// 后续需要重构为引用传递
let ast = parser.parse().clone();

// NOTE: 设计决策
// 选择使用 f64 作为唯一浮点类型，简化实现
// 这与 JavaScript/Lua 的做法一致
pub type Float = f64;

// PERF: 性能瓶颈
// 字符串拼接使用 format! 宏效率较低
// 考虑使用 String::with_capacity 预分配
let result = format!("{}{}", left, right);
```

---

## 三、AST 定义规范

### 3.1 AST 节点

Rust

```
//! 抽象语法树定义
//!
//! Xu 语言的 AST 采用"类型化节点"设计：
//! - 每种语法结构对应一个 struct
//! - 使用 enum 组合相关节点
//! - 所有节点携带 Span 信息

use crate::span::Span;

/// 程序根节点
///
/// 包含顶层语句列表
#[derive(Debug, Clone)]
pub struct Program {
    /// 顶层语句
    pub stmts: Vec<Stmt>,
    /// 整个程序的 Span
    pub span: Span,
}

/// 语句类型
#[derive(Debug, Clone)]
pub enum Stmt {
    /// 导入语句: `import "module" as alias`
    Import(ImportStmt),
    /// 类型定义: `Name with { ... }` 或 `Name with [ ... ]`
    TypeDef(TypeDef),
    /// 方法扩展: `Name does { ... }`
    DoesBlock(DoesBlock),
    /// 函数定义: `func name(...) { ... }`
    FuncDef(FuncDef),
    /// 变量声明: `let name = expr`
    Let(LetStmt),
    /// 赋值: `lvalue = expr`
    Assign(AssignStmt),
    /// 表达式语句
    Expr(ExprStmt),
    /// if 语句
    If(IfStmt),
    /// while 语句
    While(WhileStmt),
    /// for 语句
    For(ForStmt),
    /// when 语句
    When(WhenStmt),
    /// return 语句
    Return(ReturnStmt),
    /// break 语句
    Break(Span),
    /// continue 语句
    Continue(Span),
}

/// 表达式类型
#[derive(Debug, Clone)]
pub enum Expr {
    // ========== 字面量 ==========
    
    /// 整数字面量
    Int(i64, Span),
    /// 浮点数字面量
    Float(f64, Span),
    /// 字符串字面量（可能包含插值）
    String(StringExpr),
    /// 布尔字面量
    Bool(bool, Span),
    
    // ========== 标识符 ==========
    
    /// 变量引用
    Ident(String, Span),
    /// self 引用
    SelfExpr(Span),
    
    // ========== 复合字面量 ==========
    
    /// 列表: `[1, 2, 3]`
    List(ListExpr),
    /// 字典: `{"a": 1}`
    Map(MapExpr),
    /// 集合: `{1, 2, 3}` 或 `set{}`
    Set(SetExpr),
    /// 元组: `(1, "a")`
    Tuple(TupleExpr),
    /// 范围: `1..10` 或 `1..=10`
    Range(RangeExpr),
    
    // ========== 运算 ==========
    
    /// 二元运算: `a + b`
    Binary(BinaryExpr),
    /// 一元运算: `-x`, `not x`
    Unary(UnaryExpr),
    /// 逻辑运算: `a and b`, `a or b`
    Logical(LogicalExpr),
    
    // ========== 访问 ==========
    
    /// 属性访问: `obj.field`
    Field(FieldExpr),
    /// 索引访问: `list[0]`
    Index(IndexExpr),
    /// 枚举变体: `Status#pending`
    Variant(VariantExpr),
    /// 函数调用: `func(args)`
    Call(CallExpr),
    
    // ========== 其他 ==========
    
    /// Lambda: `|x| -> x * 2`
    Lambda(LambdaExpr),
    /// 结构体实例: `User{ name: "Tom" }`
    Struct(StructExpr),
    /// if 表达式: `if cond a else b`
    IfExpr(IfExpr),
    /// 分组: `(expr)`
    Group(Box<Expr>, Span),
}

impl Expr {
    /// 获取表达式的源码位置
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, span) => *span,
            Expr::Float(_, span) => *span,
            Expr::Bool(_, span) => *span,
            Expr::Ident(_, span) => *span,
            Expr::SelfExpr(span) => *span,
            Expr::String(e) => e.span,
            Expr::List(e) => e.span,
            Expr::Map(e) => e.span,
            Expr::Set(e) => e.span,
            Expr::Tuple(e) => e.span,
            Expr::Range(e) => e.span,
            Expr::Binary(e) => e.span,
            Expr::Unary(e) => e.span,
            Expr::Logical(e) => e.span,
            Expr::Field(e) => e.span,
            Expr::Index(e) => e.span,
            Expr::Variant(e) => e.span,
            Expr::Call(e) => e.span,
            Expr::Lambda(e) => e.span,
            Expr::Struct(e) => e.span,
            Expr::IfExpr(e) => e.span,
            Expr::Group(_, span) => *span,
        }
    }
}
```

### 3.2 具体节点定义

Rust

```
/// 二元表达式
///
/// 表示形如 `left op right` 的表达式
#[derive(Debug, Clone)]
pub struct BinaryExpr {
    /// 左操作数
    pub left: Box<Expr>,
    /// 运算符
    pub op: BinaryOp,
    /// 右操作数
    pub right: Box<Expr>,
    /// 源码位置
    pub span: Span,
}

/// 二元运算符
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    // 算术
    Add,      // +
    Sub,      // -
    Mul,      // *
    Div,      // /
    Mod,      // %
    
    // 比较
    Eq,       // is, ==
    Ne,       // isnt, !=
    Lt,       // <
    Gt,       // >
    Le,       // <=
    Ge,       // >=
}

/// Lambda 表达式
///
/// 表示匿名函数 `|params| -> body`
///
/// # 示例
///
/// ```xu
/// |x| -> x * 2
/// |a, b| -> a + b
/// |x| -> {
///     let y = x * 2
///     return y + 1
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LambdaExpr {
    /// 参数列表
    pub params: Vec<Param>,
    /// 函数体（表达式或块）
    pub body: LambdaBody,
    /// 源码位置
    pub span: Span,
}

/// Lambda 函数体
#[derive(Debug, Clone)]
pub enum LambdaBody {
    /// 单表达式: `|x| -> x * 2`
    Expr(Box<Expr>),
    /// 语句块: `|x| -> { ... }`
    Block(Block),
}

/// when 语句
///
/// 支持两种形式：
/// 1. 模式匹配: `when expr { Pattern { } ... }`
/// 2. 条件绑定: `when x = expr, y = expr { }`
#[derive(Debug, Clone)]
pub struct WhenStmt {
    /// when 的类型
    pub kind: WhenKind,
    /// 成功时执行的块
    pub then_block: Block,
    /// else 块（可选）
    pub else_block: Option<Block>,
    /// 源码位置
    pub span: Span,
}

/// when 语句类型
#[derive(Debug, Clone)]
pub enum WhenKind {
    /// 模式匹配
    Match {
        /// 被匹配的表达式
        expr: Box<Expr>,
        /// 匹配分支
        branches: Vec<WhenBranch>,
    },
    /// 条件绑定
    Binding {
        /// 绑定列表
        bindings: Vec<WhenBinding>,
    },
}

/// when 匹配分支
#[derive(Debug, Clone)]
pub struct WhenBranch {
    /// 匹配模式
    pub pattern: Pattern,
    /// 执行块
    pub body: Block,
    /// 源码位置
    pub span: Span,
}

/// when 条件绑定
#[derive(Debug, Clone)]
pub struct WhenBinding {
    /// 绑定的变量名
    pub name: String,
    /// 表达式
    pub expr: Box<Expr>,
    /// 源码位置
    pub span: Span,
}

/// 模式
#[derive(Debug, Clone)]
pub enum Pattern {
    /// 通配符: `_`
    Wildcard(Span),
    /// 绑定变量: `x`
    Binding(String, Span),
    /// 字面量: `1`, `"hello"`, `true`
    Literal(Expr),
    /// 元组解构: `(a, b)`
    Tuple(Vec<Pattern>, Span),
    /// 枚举匹配: `Status#pending` 或 `Result#ok(value)`
    Variant {
        /// 类型名
        type_name: String,
        /// 变体名
        variant: String,
        /// 绑定的字段（如果有）
        fields: Vec<Pattern>,
        /// 源码位置
        span: Span,
    },
}
```

---

## 四、错误处理规范

### 4.1 错误类型

Rust

```
//! 错误处理模块
//!
//! Xu 编译器使用结构化错误类型，提供友好的错误信息。

use crate::span::Span;
use std::fmt;

/// 编译错误
#[derive(Debug)]
pub struct CompileError {
    /// 错误类型
    pub kind: ErrorKind,
    /// 错误消息
    pub message: String,
    /// 源码位置
    pub span: Span,
    /// 相关提示
    pub hints: Vec<String>,
}

/// 错误类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    // 词法错误
    LexError,
    // 语法错误
    ParseError,
    // 名称解析错误
    ResolveError,
    // 类型错误
    TypeError,
    // 运行时错误
    RuntimeError,
}

impl CompileError {
    /// 创建词法错误
    pub fn lex_error(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::LexError,
            message: message.into(),
            span,
            hints: Vec::new(),
        }
    }

    /// 创建语法错误
    pub fn parse_error(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::ParseError,
            message: message.into(),
            span,
            hints: Vec::new(),
        }
    }

    /// 添加提示信息
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }

    /// 格式化错误输出
    ///
    /// 输出格式：
    /// ```text
    /// error[E0001]: 未闭合的字符串
    ///   --> main.xu:3:15
    ///    |
    ///  3 |     let msg = "hello
    ///    |               ^
    ///    |
    ///    = hint: 字符串需要用 " 结尾
    /// ```
    pub fn format(&self, source: &str, filename: &str) -> String {
        let mut output = String::new();
        
        // 错误头
        let code = match self.kind {
            ErrorKind::LexError => "L",
            ErrorKind::ParseError => "P",
            ErrorKind::ResolveError => "R",
            ErrorKind::TypeError => "T",
            ErrorKind::RuntimeError => "E",
        };
        output.push_str(&format!(
            "error[{}]: {}\n",
            code, self.message
        ));
        
        // 位置信息
        let (line, col) = self.span.line_col(source);
        output.push_str(&format!(
            "  --> {}:{}:{}\n",
            filename, line, col
        ));
        
        // 源码片段
        if let Some(line_str) = source.lines().nth(line - 1) {
            output.push_str("   |\n");
            output.push_str(&format!("{:3} | {}\n", line, line_str));
            output.push_str(&format!(
                "   | {}^\n",
                " ".repeat(col - 1)
            ));
        }
        
        // 提示信息
        for hint in &self.hints {
            output.push_str(&format!("   = hint: {}\n", hint));
        }
        
        output
    }
}
```

### 4.2 Result 类型

Rust

```
/// 编译结果类型
pub type CompileResult<T> = Result<T, CompileError>;

/// 多错误收集结果
pub type MultiResult<T> = Result<T, Vec<CompileError>>;

/// 错误收集器
///
/// 用于收集多个错误，支持继续编译收集更多错误。
#[derive(Default)]
pub struct ErrorCollector {
    errors: Vec<CompileError>,
}

impl ErrorCollector {
    /// 创建新的收集器
    pub fn new() -> Self {
        Self::default()
    }

    /// 报告错误
    pub fn report(&mut self, error: CompileError) {
        self.errors.push(error);
    }

    /// 是否有错误
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 错误数量
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// 转换为结果
    pub fn into_result<T>(self, value: T) -> MultiResult<T> {
        if self.errors.is_empty() {
            Ok(value)
        } else {
            Err(self.errors)
        }
    }
}
```

---

## 五、测试规范

### 5.1 单元测试

Rust

```
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试基本 Token 扫描
    #[test]
    fn test_scan_keywords() {
        let mut scanner = Scanner::new("if else while for");
        let tokens = scanner.scan_tokens().unwrap();
        
        assert_eq!(tokens.len(), 5); // 4 关键字 + EOF
        assert_eq!(tokens[0].kind, TokenKind::If);
        assert_eq!(tokens[1].kind, TokenKind::Else);
        assert_eq!(tokens[2].kind, TokenKind::While);
        assert_eq!(tokens[3].kind, TokenKind::For);
    }

    /// 测试字符串插值
    #[test]
    fn test_string_interpolation() {
        let mut scanner = Scanner::new(r#""Hello, {name}!""#);
        let tokens = scanner.scan_tokens().unwrap();
        
        assert_eq!(tokens.len(), 2);
        match &tokens[0].kind {
            TokenKind::String(s) => {
                assert!(s.contains("{name}"));
            }
            _ => panic!("期望字符串 Token"),
        }
    }

    /// 测试数字字面量
    #[test]
    fn test_numbers() {
        let cases = vec![
            ("42", TokenKind::Int(42)),
            ("0xFF", TokenKind::Int(255)),
            ("0b1010", TokenKind::Int(10)),
            ("1_000_000", TokenKind::Int(1_000_000)),
            ("3.14", TokenKind::Float(3.14)),
            ("1.5e10", TokenKind::Float(1.5e10)),
        ];

        for (input, expected) in cases {
            let mut scanner = Scanner::new(input);
            let tokens = scanner.scan_tokens().unwrap();
            assert_eq!(tokens[0].kind, expected, "输入: {}", input);
        }
    }

    /// 测试错误恢复
    #[test]
    fn test_error_recovery() {
        let mut scanner = Scanner::new("let x = \"unclosed");
        let result = scanner.scan_tokens();
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("未闭合"));
    }
}
```

### 5.2 集成测试

Rust

```
// tests/parser_tests.rs

use xu_lang::parser::Parser;
use xu_lang::lexer::Scanner;

/// 测试完整程序解析
#[test]
fn test_parse_program() {
    let source = r#"
        User with {
            name: string
            age: int = 0
            
            func greet() {
                println("Hi, {self.name}")
            }
        }
        
        func main() {
            let user = User{ name: "Tom" }
            user.greet()
        }
    "#;
    
    let mut scanner = Scanner::new(source);
    let tokens = scanner.scan_tokens().unwrap();
    
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    
    assert_eq!(program.stmts.len(), 2);
}

/// 测试 when 模式匹配
#[test]
fn test_parse_when_match() {
    let source = r#"
        when status {
            Status#pending { println("待处理") }
            Status#approved { println("已通过") }
        } else {
            println("其他")
        }
    "#;
    
    let program = parse(source).unwrap();
    
    match &program.stmts[0] {
        Stmt::When(when_stmt) => {
            match &when_stmt.kind {
                WhenKind::Match { branches, .. } => {
                    assert_eq!(branches.len(), 2);
                }
                _ => panic!("期望模式匹配"),
            }
        }
        _ => panic!("期望 when 语句"),
    }
}

/// 测试 when 条件绑定
#[test]
fn test_parse_when_binding() {
    let source = r#"
        when user = find_user(id), profile = get_profile(user.id) {
            use(user, profile)
        }
    "#;
    
    let program = parse(source).unwrap();
    
    match &program.stmts[0] {
        Stmt::When(when_stmt) => {
            match &when_stmt.kind {
                WhenKind::Binding { bindings } => {
                    assert_eq!(bindings.len(), 2);
                    assert_eq!(bindings[0].name, "user");
                    assert_eq!(bindings[1].name, "profile");
                }
                _ => panic!("期望条件绑定"),
            }
        }
        _ => panic!("期望 when 语句"),
    }
}
```

---

## 六、代码风格

### 6.1 命名约定

Rust

```
// 类型名: PascalCase
pub struct TokenKind { }
pub enum Expr { }
pub trait Visitor { }

// 函数/方法: snake_case
pub fn scan_tokens(&mut self) { }
fn is_at_end(&self) -> bool { }

// 常量: SCREAMING_SNAKE_CASE
pub const MAX_PARAMS: usize = 255;
pub const VERSION: &str = "1.0.0";

// 模块: snake_case
mod lexer;
mod parser;
mod type_checker;

// 生命周期: 小写简短
impl<'a> Scanner<'a> { }
fn parse<'src>(source: &'src str) { }
```

### 6.2 代码组织

Rust

```
// 导入顺序: std -> 外部 crate -> 本地模块
use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CompileError;
use crate::span::Span;

// 结构体字段顺序: 重要字段在前
pub struct Token {
    pub kind: TokenKind,    // 主要信息
    pub span: Span,         // 位置信息
}

// impl 块顺序: new -> 公共方法 -> 私有方法
impl Scanner {
    // 构造函数
    pub fn new(source: &str) -> Self { }
    
    // 公共 API
    pub fn scan_tokens(&mut self) -> Result<Vec<Token>> { }
    
    // 私有辅助方法
    fn scan_token(&mut self) { }
    fn advance(&mut self) -> char { }
}
```

### 6.3 错误处理

Rust

```
// 使用 ? 操作符传播错误
pub fn scan_tokens(&mut self) -> Result<Vec<Token>, LexError> {
    while !self.is_at_end() {
        self.scan_token()?;
    }
    Ok(self.tokens.clone())
}

// 提供有意义的错误信息
fn scan_string(&mut self) -> Result<(), LexError> {
    if self.is_at_end() {
        return Err(LexError::new(
            "未闭合的字符串",
            self.current_span(),
        ).with_hint("字符串需要用 \" 结尾"));
    }
    // ...
}

// 使用 Option 表示可选值
fn peek_next(&self) -> Option<char> {
    self.source.get(self.current + 1).copied()
}
```

---

## 七、性能指南

### 7.1 避免不必要的分配

Rust

```
// ❌ 不好: 每次调用都分配新 String
fn get_keyword(name: &str) -> Option<TokenKind> {
    let keywords = HashMap::from([
        ("if".to_string(), TokenKind::If),
        // ...
    ]);
    keywords.get(name).copied()
}

// ✅ 好: 使用静态哈希表
use once_cell::sync::Lazy;

static KEYWORDS: Lazy<HashMap<&'static str, TokenKind>> = Lazy::new(|| {
    HashMap::from([
        ("if", TokenKind::If),
        ("else", TokenKind::Else),
        // ...
    ])
});

fn get_keyword(name: &str) -> Option<TokenKind> {
    KEYWORDS.get(name).copied()
}
```

### 7.2 使用迭代器

Rust

```
// ❌ 不好: 中间 Vec 分配
fn find_functions(stmts: &[Stmt]) -> Vec<&FuncDef> {
    let mut result = Vec::new();
    for stmt in stmts {
        if let Stmt::FuncDef(f) = stmt {
            result.push(f);
        }
    }
    result
}

// ✅ 好: 返回迭代器
fn find_functions(stmts: &[Stmt]) -> impl Iterator<Item = &FuncDef> {
    stmts.iter().filter_map(|stmt| {
        if let Stmt::FuncDef(f) = stmt {
            Some(f)
        } else {
            None
        }
    })
}
```

### 7.3 预分配容量

Rust

```
// ❌ 不好: 多次重新分配
fn collect_tokens(&mut self) -> Vec<Token> {
    let mut tokens = Vec::new();
    while !self.is_at_end() {
        tokens.push(self.next_token());
    }
    tokens
}

// ✅ 好: 预估容量
fn collect_tokens(&mut self) -> Vec<Token> {
    // 粗略估计: 每 5 个字符一个 Token
    let estimated = self.source.len() / 5;
    let mut tokens = Vec::with_capacity(estimated);
    while !self.is_at_end() {
        tokens.push(self.next_token());
    }
    tokens
}
```

---

## 变更记录

|版本|主要变更|
|---|---|
|v1.0|初始开发规范|
||完整注释规范|
||AST 定义规范|
||错误处理规范|
||测试规范|
||代码风格指南|

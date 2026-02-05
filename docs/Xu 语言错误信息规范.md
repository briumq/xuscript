# Xu 语言错误信息规范

---

## 一、设计原则

|原则|说明|
|---|---|
|**精确定位**|准确指向错误位置，显示源码上下文|
|**清晰描述**|用简洁的语言说明问题|
|**提供建议**|给出可能的修复方案|
|**一致格式**|所有错误遵循统一格式|
|**渐进详细**|支持 `--verbose` 显示更多信息|

---

## 二、错误分类与编码

### 2.1 错误码格式

```
[级别][类别][序号]

级别:
  E = Error (错误，阻止编译/运行)
  W = Warning (警告，代码质量问题)
  I = Info (提示信息)

类别 (4位数字):
  0xxx = 通用/标识符
  1xxx = 类型系统
  2xxx = 语法/解析
  3xxx = 运行时
  4xxx = 导入/模块
  5xxx = 方法/成员
```

### 2.2 Severity 级别

|级别|说明|退出码|
|---|---|---|
|Error|阻止编译/运行的严重错误|1|
|Warning|代码质量问题，不阻止运行|0|
|Info|提示信息，如废弃 API 提醒|0|

### 2.3 错误分类

|类别码|含义|Error 范围|Warning 范围|
|---|---|---|---|
|0xxx|通用/标识符|E0001-E0099|W0001-W0099|
|1xxx|类型系统|E1001-E1099|-|
|2xxx|语法/解析|E2001-E2099|-|
|3xxx|运行时|E3001-E3099|-|
|4xxx|导入/模块|E4001-E4099|-|
|5xxx|方法/成员|E5001-E5099|-|

### 2.4 已实现的错误代码

#### Errors (E)

| 代码 | 名称 | 说明 |
|------|------|------|
| E0001 | UNDEFINED_IDENTIFIER | 未定义标识符 |
| E1001 | TYPE_MISMATCH | 类型不匹配 |
| E1002 | ARGUMENT_COUNT_MISMATCH | 参数数量错误 |
| E1003 | RETURN_TYPE_MISMATCH | 返回类型错误 |
| E1004 | INVALID_CONDITION_TYPE | 无效条件类型 |
| E1005 | INVALID_ITERATOR_TYPE | 无效迭代器类型 |
| E1006 | INVALID_UNARY_OPERAND | 无效一元操作数 |
| E2001 | EXPECTED_TOKEN | 期望的 token |
| E2002 | EXPECTED_EXPRESSION | 期望表达式 |
| E2003 | INVALID_ASSIGNMENT_TARGET | 无效赋值目标 |
| E2004 | UNTERMINATED_STRING | 未终止字符串 |
| E2005 | UNTERMINATED_BLOCK_COMMENT | 未终止注释 |
| E2006 | UNEXPECTED_CHAR | 意外字符 |
| E2007 | UNCLOSED_DELIMITER | 未闭合分隔符 |
| E2008 | KEYWORD_AS_IDENTIFIER | 关键字不能作为标识符 |
| E3001 | INDEX_OUT_OF_RANGE | 索引越界 |
| E3002 | DIVISION_BY_ZERO | 除零错误 |
| E3003 | KEY_NOT_FOUND | 键不存在 |
| E3004 | INTEGER_OVERFLOW | 整数溢出 |
| E3005 | RECURSION_LIMIT_EXCEEDED | 递归限制超出 |
| E3006 | NOT_CALLABLE | 不可调用 |
| E4001 | CIRCULAR_IMPORT | 循环导入 |
| E4002 | IMPORT_FAILED | 导入失败 |
| E4003 | FILE_NOT_FOUND | 文件未找到 |
| E4004 | PATH_NOT_ALLOWED | 路径不允许 |
| E5001 | UNKNOWN_STRUCT | 未知结构体 |
| E5002 | UNKNOWN_MEMBER | 未知成员 |
| E5003 | UNKNOWN_ENUM_VARIANT | 未知枚举变体 |
| E5004 | UNSUPPORTED_METHOD | 不支持的方法 |
| E5005 | INVALID_MEMBER_ACCESS | 无效成员访问 |

#### Warnings (W)

| 代码 | 名称 | 说明 |
|------|------|------|
| W0001 | UNREACHABLE_CODE | 不可达代码 |
| W0002 | SHADOWING | 变量遮蔽 |
| W0003 | UNIT_ASSIGNMENT | unit 赋值 |

---

## 三、错误输出格式

### 3.1 标准格式

```
{级别} [{错误码}]:{行}:{列}: {文件}: {错误描述}
  | {源代码}
  | {指向错误位置}
```

### 3.2 示例

```
Error [E0001]:5:10: main.xu: Undefined identifier: user
  |     println(user.name)
  |             ^^^^
```

```
Warning [W0002]:4:9: main.xu: Variable 's' shadows an existing binding
  |     let s = 2
  |         ^
```

```
Error [E1001]:7:14: main.xu: Type mismatch: expected int but got string
  |     let x: int = "hello"
  |                  ^^^^^^^
```

### 3.3 多行错误

text

```
error[P025]: when 语句未穷尽
  --> main.xu:10:5
   |
10 | /     when status {
11 | |         Status#pending { println("待处理") }
12 | |     }
   | |_____^ 缺少分支
   |
   = help: 添加缺失的分支或 else 块
   = note: 缺少: Status#approved, Status#rejected
```

### 3.4 关联错误

text

```
error[R002]: 未定义的变量 'user'
  --> main.xu:8:14
   |
 8 |     println(user.name)
   |             ^^^^ 未定义
   |
   = help: 是否想用 'users'?
   
note: 相似的名称定义在这里
  --> main.xu:3:9
   |
 3 |     let users = []
   |         ^^^^^ 
```

---

## 四、词法错误 (L001-L099)

### L001: 未闭合的字符串

text

```
error[L001]: 未闭合的字符串
  --> {file}:{line}:{col}
   |
 {line} |     let s = "hello
   |             ^ 字符串从这里开始
   |
   = help: 用 " 结束字符串
   = note: 多行字符串使用 """..."""
```

**触发条件**: 字符串开始后未找到结束引号

### L002: 无效的转义序列

text

```
error[L002]: 无效的转义序列 '\q'
  --> {file}:{line}:{col}
   |
 {line} |     let s = "hello\qworld"
   |                       ^^ 无效转义
   |
   = help: 有效的转义: \n \t \r \\ \" \{
   = note: 如需字面反斜杠，使用 \\ 或原始字符串 r"..."
```

**触发条件**: `\` 后跟非法字符

### L003: 未知字符

text

```
error[L003]: 未知字符 '@'
  --> {file}:{line}:{col}
   |
 {line} |     let x = @value
   |             ^ 无法识别
   |
   = help: Xu 不支持 @ 符号
```

**触发条件**: 遇到不属于任何 Token 的字符

### L004: 无效的数字格式

text

```
error[L004]: 无效的十六进制数字
  --> {file}:{line}:{col}
   |
 {line} |     let x = 0xGG
   |                 ^^^^ 期望 0-9, a-f, A-F
   |
   = help: 十六进制格式: 0x[0-9a-fA-F]+
```

**触发条件**: 数字字面量格式错误

### L005: 未闭合的多行注释

text

```
error[L005]: 未闭合的多行注释
  --> {file}:{line}:{col}
   |
 {line} |     /* comment
   |     ^^ 注释从这里开始
   |
   = help: 用 */ 结束多行注释
```

**触发条件**: `/*` 未找到匹配的 `*/`

### L006: 未闭合的字符串插值

text

```
error[L006]: 未闭合的字符串插值
  --> {file}:{line}:{col}
   |
 {line} |     let s = "Hello, {name"
   |                         ^ 插值从这里开始
   |
   = help: 用 } 结束插值表达式
```

**触发条件**: 字符串内 `{` 未找到匹配的 `}`

### L007: 意外的字符

text

```
error[L007]: 意外的字符 '!'
  --> {file}:{line}:{col}
   |
 {line} |     if x ! y { }
   |            ^ 
   |
   = help: 是否想用 'isnt' 或 '!='?
```

**触发条件**: `!` 单独出现（不是 `!=`）

---

## 五、语法错误 (P001-P199)

### P001: 期望表达式

text

```
error[P001]: 期望表达式
  --> {file}:{line}:{col}
   |
 {line} |     let x = 
   |               ^ 这里需要一个表达式
   |
   = help: 提供一个值，如 0, "", [] 等
```

### P002: 期望标识符

text

```
error[P002]: 期望标识符
  --> {file}:{line}:{col}
   |
 {line} |     let 123 = 1
   |         ^^^ 期望变量名
   |
   = help: 标识符以字母或下划线开头
```

### P002b: 关键字不能作为标识符

text

```
error[E2008]: 关键字不能作为标识符
  --> {file}:{line}:{col}
   |
 {line} |     func pub() { }
   |          ^^^ 'pub' 是保留关键字
   |
   = help: 选择其他名称，如 'pub_func' 或 '_pub'
   = note: 保留关键字: if, else, for, while, func, return, break, continue,
           let, var, match, when, has, does, pub, static, self, use, as,
           is, with, can, async, await, true, false
```

**触发条件**: 在需要标识符的位置使用了关键字

### P003: 期望类型

text

```
error[P003]: 期望类型
  --> {file}:{line}:{col}
   |
 {line} |     let x: = 1
   |            ^ 期望类型名
   |
   = help: 如 int, string, [int], {string: int}
```

### P010: 期望 ')'

text

```
error[P010]: 期望 ')'
  --> {file}:{line}:{col}
   |
 {line} |     let x = (1 + 2
   |                      ^ 期望 ')' 来匹配
   |
 {line-1} |     let x = (1 + 2
   |             ^ 这个 '('
```

### P011: 期望 '}'

text

```
error[P011]: 期望 '}'
  --> {file}:{line}:{col}
   |
 {line} |     if x > 0 {
   |              ^ 期望 '}' 来匹配这个 '{'
```

### P012: 变量声明缺少初始化

text

```
error[P012]: 变量声明缺少初始化
  --> {file}:{line}:{col}
   |
 {line} |     let x
   |         ^^^^^ 需要 = 和初始值
   |
   = help: Xu 不允许未初始化的变量
   = note: 尝试 `let x = 0` 或 `let x = ""`
```

### P013: 意外的 Token

text

```
error[P013]: 意外的 token '+'
  --> {file}:{line}:{col}
   |
 {line} |     let x = + 1
   |             ^ 期望表达式
   |
   = help: 移除多余的 '+'
```

### P020: 函数缺少参数列表

text

```
error[P020]: 函数缺少参数列表
  --> {file}:{line}:{col}
   |
 {line} |     func foo {
   |              ^^^ 期望 '('
   |
   = help: 使用 `func foo() { }` 或 `func foo(a: int) { }`
```

### P021: 函数缺少函数体

text

```
error[P021]: 函数缺少函数体
  --> {file}:{line}:{col}
   |
 {line} |     func foo()
   |                  ^ 期望 '{'
   |
   = help: 添加函数体 `{ ... }`
```

### P022: 参数缺少类型标注

text

```
error[P022]: 参数缺少类型标注
  --> {file}:{line}:{col}
   |
 {line} |     func foo(x) { }
   |              ^ 需要类型
   |
   = help: 使用 `func foo(x: int) { }`
```

### P025: when 未穷尽

text

```
error[P025]: when 语句未穷尽
  --> {file}:{line}:{col}
   |
 {line} | /     when status {
   | |         Status#pending { }
   | |     }
   |         ^ 缺少分支
   |
   = help: 添加 else 块或补全所有分支
   = note: 缺少: Status#approved, Status#rejected
```

### P026: when 重复分支

text

```
error[P026]: when 语句重复分支
  --> {file}:{line}:{col}
   |
 8 |         Status#pending { println("A") }
   |         -------------- 首次出现
 9 |         Status#pending { println("B") }
   |         ^^^^^^^^^^^^^^ 重复
   |
   = help: 移除重复的分支
```

### P030: 结构体字段缺少类型

text

```
error[P030]: 结构体字段缺少类型
  --> {file}:{line}:{col}
   |
 {line} |     User with { name }
   |                 ^^^^ 需要类型标注
   |
   = help: 使用 `name: string`
```

### P031: 结构体重复字段

text

```
error[P031]: 结构体重复字段 'name'
  --> {file}:{line}:{col}
   |
 3 |     name: string
   |     ---- 首次定义
 4 |     name: int
   |     ^^^^ 重复定义
```

### P040: 枚举缺少变体

text

```
error[P040]: 枚举缺少变体
  --> {file}:{line}:{col}
   |
 {line} |     Status with []
   |                   ^^ 枚举至少需要一个变体
```

### P050: （已废弃）

> 此错误码已废弃。`inner` 关键字已被移除，现在使用 `pub` 标记公开成员，默认为私有。

### P060: return 在函数外

text

```
error[P060]: return 语句在函数外部
  --> {file}:{line}:{col}
   |
 {line} |     return 42
   |     ^^^^^^ 只能在函数内使用
```

### P061: break 在循环外

text

```
error[P061]: break 语句在循环外部
  --> {file}:{line}:{col}
   |
 {line} |     break
   |     ^^^^^ 只能在 while/for 循环内使用
```

### P062: continue 在循环外

text

```
error[P062]: continue 语句在循环外部
  --> {file}:{line}:{col}
   |
 {line} |     continue
   |     ^^^^^^^^ 只能在 while/for 循环内使用
```

---

## 六、名称解析错误 (R001-R099)

### R001: 未定义的变量

text

```
error[R001]: 未定义的变量 'user'
  --> {file}:{line}:{col}
   |
 {line} |     println(user.name)
   |             ^^^^ 未定义
   |
   = help: 检查拼写或先声明变量
```

**带建议版本**:

text

```
error[R001]: 未定义的变量 'user'
  --> {file}:{line}:{col}
   |
 {line} |     println(user.name)
   |             ^^^^ 未定义
   |
   = help: 是否想用 'users'?

note: 相似的名称在这里定义
  --> main.xu:3:9
   |
 3 |     let users = []
   |         ^^^^^
```

### R002: 未定义的函数

text

```
error[R002]: 未定义的函数 'prnt'
  --> {file}:{line}:{col}
   |
 {line} |     prnt("hello")
   |     ^^^^ 未定义
   |
   = help: 是否想用 'print' 或 'println'?
```

### R003: 未定义的类型

text

```
error[R003]: 未定义的类型 'Usr'
  --> {file}:{line}:{col}
   |
 {line} |     let u: Usr = ...
   |            ^^^ 未定义
   |
   = help: 是否想用 'User'?
```

### R004: 未定义的字段

text

```
error[R004]: 类型 'User' 没有字段 'nme'
  --> {file}:{line}:{col}
   |
 {line} |     println(user.nme)
   |                      ^^^ 未定义
   |
   = help: User 的字段: name, age, status
   = note: 是否想用 'name'?
```

### R005: 未定义的方法

text

```
error[R005]: 类型 'User' 没有方法 'gret'
  --> {file}:{line}:{col}
   |
 {line} |     user.gret()
   |          ^^^^ 未定义
   |
   = help: User 的方法: greet(), is_adult()
   = note: 是否想用 'greet'?
```

### R006: 未定义的枚举变体

text

```
error[R006]: 枚举 'Status' 没有变体 'done'
  --> {file}:{line}:{col}
   |
 {line} |     let s = Status#done
   |                        ^^^^ 未定义
   |
   = help: Status 的变体: pending, approved, rejected
```

### R010: 重复定义

text

```
error[R010]: 重复定义 'add'
  --> {file}:{line}:{col}
   |
 3 |     func add(a: int, b: int) -> int { ... }
   |          --- 首次定义
...
 8 |     func add(x: int) -> int { ... }
   |          ^^^ 重复定义
   |
   = help: 重命名其中一个函数
```

### R011: 变量遮蔽警告

text

```
warning[R011]: 变量 'x' 遮蔽了外层定义
  --> {file}:{line}:{col}
   |
 3 |     let x = 1
   |         - 外层定义
...
 5 |         let x = 2
   |             ^ 遮蔽
   |
   = note: 这是允许的，但可能不是你想要的
```

### R020: 模块未找到

text

```
error[R020]: 模块未找到 'utilss'
  --> {file}:{line}:{col}
   |
 1 |     import "utilss"
   |            ^^^^^^^^ 找不到
   |
   = help: 检查文件是否存在: utilss.xu
   = note: 是否想用 'utils'?
```

### R021: 循环导入

text

```
error[R021]: 循环导入
  --> {file}:{line}:{col}
   |
   = note: a.xu -> b.xu -> c.xu -> a.xu
   |
   = help: 重构模块依赖，打破循环
```

---

## 七、类型错误 (T001-T199)

### T001: 类型不匹配

text

```
error[T001]: 类型不匹配
  --> {file}:{line}:{col}
   |
 {line} |     let x: int = "hello"
   |            ---   ^^^^^^^ 发现 string
   |            |
   |            期望 int
   |
   = help: 提供 int 类型的值或移除类型标注
```

### T002: 参数类型不匹配

text

```
error[T002]: 参数类型不匹配
  --> {file}:{line}:{col}
   |
 {line} |     add("hello", 2)
   |         ^^^^^^^ 期望 int，发现 string
   |
note: 函数定义在这里
  --> main.xu:1:1
   |
 1 |     func add(a: int, b: int) -> int
   |              ^^^^^^
```

### T003: 返回类型不匹配

text

```
error[T003]: 返回类型不匹配
  --> {file}:{line}:{col}
   |
 {line} |     func foo() -> int {
   |                    --- 期望返回 int
...
 {line+2} |         return "hello"
   |                ^^^^^^^ 发现 string
```

### T004: 条件必须是布尔

text

```
error[T004]: 条件表达式必须是 bool
  --> {file}:{line}:{col}
   |
 {line} |     if x + 1 {
   |        ^^^^^ 发现 int
   |
   = help: 使用比较表达式，如 `x + 1 > 0`
   = note: Xu 不支持隐式转换为 bool
```

### T005: 无法推断类型

text

```
error[T005]: 无法推断空集合的类型
  --> {file}:{line}:{col}
   |
 {line} |     let list = []
   |             ^^^^ 类型未知
   |
   = help: 添加类型标注 `let list: [int] = []`
   = note: 或稍后添加元素让编译器推断类型
```

### T010: 运算符类型错误

text

```
error[T010]: 运算符 '+' 不能用于 string 和 int
  --> {file}:{line}:{col}
   |
 {line} |     let x = "hello" + 42
   |                 ^^^^^^^ ^ ^^
   |                 string    int
   |
   = help: 使用字符串插值 `"hello{42}"` 或转换类型
```

### T011: 无法比较

text

```
error[T011]: 类型 'User' 不支持比较运算
  --> {file}:{line}:{col}
   |
 {line} |     if user1 < user2 {
   |            ^^^^^^^^^^^^^^ 无法比较
   |
   = help: 比较具体字段，如 `user1.age < user2.age`
```

### T020: 不是函数

text

```
error[T020]: 'x' 不是函数
  --> {file}:{line}:{col}
   |
 {line} |     x(1, 2)
   |     ^ 期望函数，发现 int
   |
   = help: 检查变量名或定义函数
```

### T021: 参数数量不匹配

text

```
error[T021]: 参数数量不匹配
  --> {file}:{line}:{col}
   |
 {line} |     add(1)
   |         ^^^ 期望 2 个参数，发现 1 个
   |
note: 函数定义
   |
 1 |     func add(a: int, b: int) -> int
   |              ^^^^^^  ^^^^^^
```

### T022: 多余的参数

text

```
error[T022]: 多余的参数
  --> {file}:{line}:{col}
   |
 {line} |     add(1, 2, 3)
   |               ^ 多余
   |
   = note: add 只接受 2 个参数
```

### T030: 不是结构体

text

```
error[T030]: 'int' 不是结构体类型
  --> {file}:{line}:{col}
   |
 {line} |     let x = int{ value: 1 }
   |             ^^^ 不能实例化
```

### T031: 缺少必需字段

text

```
error[T031]: 缺少必需字段 'name'
  --> {file}:{line}:{col}
   |
 {line} |     let u = User{ age: 20 }
   |                 ^^^^ 缺少 'name'
   |
note: User 定义
   |
 1 |     User with {
 2 |         name: string    // 无默认值，必需
 3 |         age: int = 0    // 有默认值，可选
   |     }
```

### T032: 未知字段

text

```
error[T032]: 未知字段 'email'
  --> {file}:{line}:{col}
   |
 {line} |     let u = User{ name: "Tom", email: "..." }
   |                                 ^^^^^ User 没有此字段
   |
   = help: User 的字段: name, age, status
```

### T040: 不是枚举

text

```
error[T040]: 'User' 不是枚举类型
  --> {file}:{line}:{col}
   |
 {line} |     let x = User#pending
   |                     ^ 不能使用 # 访问
   |
   = help: # 用于枚举变体，如 Status#pending
```

### T050: Option/Result 未处理

text

```
warning[T050]: Option 值未处理
  --> {file}:{line}:{col}
   |
 {line} |     list.first
   |     ^^^^^^^^^^ 返回 Option，但结果被忽略
   |
   = help: 使用 when 绑定或 .or() 提供默认值
```

### T051: 非 Option 使用 Option 算子

text

```
error[T051]: 'int' 不是 Option 类型
  --> {file}:{line}:{col}
   |
 {line} |     let x = 42.or(0)
   |                    ^^ int 没有 .or() 方法
   |
   = note: .or() 用于 Option 和 Result 类型
```

---

## 八、运行时错误 (E001-E199)

### E001: 索引越界

text

```
error[E001]: 索引越界
  --> {file}:{line}:{col}
   |
 {line} |     let x = list[10]
   |                      ^^ 索引 10 超出范围
   |
   = note: 列表长度为 3，有效索引: 0..2 或 -3..-1
   = help: 使用 .get(10) 返回 Option 避免 panic
```

### E002: 键不存在

text

```
error[E002]: 键不存在
  --> {file}:{line}:{col}
   |
 {line} |     let x = map["unknown"]
   |                     ^^^^^^^^^ 键 "unknown" 不存在
   |
   = help: 使用 .get("unknown") 返回 Option
   = note: 或先用 .has("unknown") 检查
```

### E003: 除零错误

text

```
error[E003]: 除零错误
  --> {file}:{line}:{col}
   |
 {line} |     let x = 10 / 0
   |                    ^ 除数为零
   |
   = help: 在除法前检查除数
```

### E004: 断言失败

text

```
error[E004]: 断言失败
  --> {file}:{line}:{col}
   |
 {line} |     assert(x > 0, "x must be positive")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: x must be positive
   = note: 实际值: x = -5
```

### E005: panic

text

```
error[E005]: panic
  --> {file}:{line}:{col}
   |
 {line} |     panic("something went wrong")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: something went wrong
```

### E010: 栈溢出

text

```
error[E010]: 栈溢出
  --> {file}:{line}:{col}
   |
   = note: 递归调用过深
   = note: 调用栈:
           main.xu:10 infinite()
           main.xu:10 infinite()
           main.xu:10 infinite()
           ... (997 more)
   |
   = help: 检查递归终止条件
```

### E011: 类型转换失败

text

```
error[E011]: 类型转换失败
  --> {file}:{line}:{col}
   |
 {line} |     let n = int("hello")
   |                     ^^^^^^^ 无法转换为 int
   |
   = help: int() 返回 Result，使用 .or(0) 提供默认值
```

### E020: 文件不存在

text

```
error[E020]: 文件不存在
  --> {file}:{line}:{col}
   |
 {line} |     file.read("config.json")
   |                   ^^^^^^^^^^^^^^ 文件不存在
   |
   = note: 路径: /home/user/project/config.json
   = help: 检查文件路径或使用 file.exists() 先检查
```

### E021: 权限拒绝

text

```
error[E021]: 权限拒绝
  --> {file}:{line}:{col}
   |
 {line} |     file.write("/etc/passwd", "...")
   |                    ^^^^^^^^^^^^^ 无写入权限
```

### E030: 网络错误

text

```
error[E030]: 网络错误
  --> {file}:{line}:{col}
   |
 {line} |     http.get("https://example.com/api")
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: 连接超时
   = help: 检查网络连接或增加超时时间
```

### E040: JSON 解析失败

text

```
error[E040]: JSON 解析失败
  --> {file}:{line}:{col}
   |
 {line} |     json.parse(content)
   |                    ^^^^^^^ 无效的 JSON
   |
   = note: 第 3 行第 15 列: 期望 ',' 或 '}'
```

---

## 九、警告 (W001-W099)

### W001: 未使用的变量

text

```
warning[W001]: 未使用的变量 'x'
  --> {file}:{line}:{col}
   |
 {line} |     let x = 1
   |         ^ 从未使用
   |
   = help: 如果是有意的，使用 _x 或 _
```

### W002: 未使用的函数

text

```
warning[W002]: 未使用的函数 'helper'
  --> {file}:{line}:{col}
   |
 {line} |     func helper() { }
   |          ^^^^^^ 从未调用
   |
   = help: 如果是内部函数，保持默认私有即可（不添加 pub）
```

### W003: 未使用的导入

text

```
warning[W003]: 未使用的导入 'math'
  --> {file}:{line}:{col}
   |
 {line} |     import "math"
   |            ^^^^^^ 从未使用
```

### W010: 不可达代码

text

```
warning[W010]: 不可达代码
  --> {file}:{line}:{col}
   |
 {line} |         return x
 {line+1} |         println("unreachable")
   |         ^^^^^^^^^^^^^^^^^^^^^^ 不会执行
```

### W011: 条件总是真/假

text

```
warning[W011]: 条件总是为真
  --> {file}:{line}:{col}
   |
 {line} |     if true { }
   |        ^^^^ 总是为真
   |
   = help: 移除条件或检查逻辑
```

### W020: 废弃用法

text

```
warning[W020]: '==' 已废弃，使用 'is'
  --> {file}:{line}:{col}
   |
 {line} |     if x == 1 { }
   |            ^^ 建议使用 'is'
   |
   = help: 替换为 `x is 1`
```

---

## 十、错误消息模板

### 10.1 Rust 实现

Rust

```
/// 错误消息模板
pub struct ErrorTemplate {
    /// 错误码
    pub code: &'static str,
    /// 消息模板（支持 {0}, {1} 占位符）
    pub message: &'static str,
    /// 帮助信息
    pub help: Option<&'static str>,
    /// 补充说明
    pub note: Option<&'static str>,
}

/// 错误模板定义
pub mod templates {
    use super::ErrorTemplate;

    pub const L001: ErrorTemplate = ErrorTemplate {
        code: "L001",
        message: "未闭合的字符串",
        help: Some("用 \" 结束字符串，或使用 \"\"\" 多行字符串"),
        note: None,
    };

    pub const P012: ErrorTemplate = ErrorTemplate {
        code: "P012",
        message: "变量声明缺少初始化",
        help: Some("Xu 要求变量必须初始化"),
        note: Some("尝试 `let x = 0` 或 `let x = \"\"`"),
    };

    pub const T001: ErrorTemplate = ErrorTemplate {
        code: "T001",
        message: "类型不匹配：期望 {0}，发现 {1}",
        help: None,
        note: None,
    };

    pub const R001: ErrorTemplate = ErrorTemplate {
        code: "R001",
        message: "未定义的变量 '{0}'",
        help: None,
        note: None,
    };

    pub const E001: ErrorTemplate = ErrorTemplate {
        code: "E001",
        message: "索引越界：索引 {0} 超出范围",
        help: Some("使用 .get({0}) 返回 Option 避免 panic"),
        note: None,
    };
}
```

### 10.2 错误生成器

Rust

```
impl CompileError {
    /// 生成未定义变量错误
    pub fn undefined_variable(name: &str, span: Span, similar: Option<&str>) -> Self {
        let mut err = Self::new(
            ErrorKind::Resolve,
            format!("未定义的变量 '{}'", name),
            span,
        );
        
        if let Some(sim) = similar {
            err = err.with_help(format!("是否想用 '{}'?", sim));
        } else {
            err = err.with_help("检查拼写或先声明变量");
        }
        
        err
    }

    /// 生成类型不匹配错误
    pub fn type_mismatch(expected: &Type, found: &Type, span: Span) -> Self {
        Self::new(
            ErrorKind::Type,
            format!("类型不匹配：期望 {}，发现 {}", expected, found),
            span,
        )
    }

    /// 生成索引越界错误
    pub fn index_out_of_bounds(index: i64, length: usize, span: Span) -> Self {
        let valid_range = if length > 0 {
            format!("0..{} 或 -{}..0", length - 1, length)
        } else {
            "列表为空".to_string()
        };
        
        Self::new(
            ErrorKind::Runtime,
            format!("索引越界：索引 {} 超出范围", index),
            span,
        )
        .with_note(format!("列表长度为 {}，有效索引: {}", length, valid_range))
        .with_help(format!("使用 .get({}) 返回 Option 避免 panic", index))
    }
}
```

---

## 十一、相似名称建议算法

### 11.1 编辑距离

Rust

```
/// 计算两个字符串的编辑距离（Levenshtein）
fn edit_distance(a: &str, b: &str) -> usize {
    let m = a.len();
    let n = b.len();
    
    let mut dp = vec![vec![0; n + 1]; m + 1];
    
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    
    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            dp[i + 1][j + 1] = (dp[i][j + 1] + 1)
                .min(dp[i + 1][j] + 1)
                .min(dp[i][j] + cost);
        }
    }
    
    dp[m][n]
}

/// 查找相似名称
fn find_similar<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let threshold = match name.len() {
        1 => 0,
        2..=3 => 1,
        4..=6 => 2,
        _ => 3,
    };
    
    candidates
        .iter()
        .map(|c| (c, edit_distance(name, c)))
        .filter(|(_, d)| *d <= threshold)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| *c)
}
```

### 11.2 使用示例

Rust

```
// 查找变量时
if let Some(similar) = find_similar("usr", &["user", "users", "username"]) {
    // similar = "user"
    error.with_help(format!("是否想用 '{}'?", similar));
}
```

---

## 十二、错误输出级别

### 12.1 命令行选项

Bash

```
# 默认：错误和警告
xu run main.xu

# 只显示错误
xu run main.xu --no-warnings

# 详细模式
xu run main.xu --verbose

# 指定警告级别
xu run main.xu -W error    # 警告作为错误
xu run main.xu -W none     # 禁用警告

# JSON 输出（IDE 集成）
xu run main.xu --error-format=json
```

### 12.2 JSON 格式

JSON

```
{
  "errors": [
    {
      "code": "T001",
      "level": "error",
      "message": "类型不匹配：期望 int，发现 string",
      "file": "main.xu",
      "line": 7,
      "column": 13,
      "end_line": 7,
      "end_column": 20,
      "help": "提供 int 类型的值或移除类型标注",
      "source": "    let x: int = \"hello\""
    }
  ],
  "warnings": [],
  "error_count": 1,
  "warning_count": 0
}
```

---

## 十三、颜色输出

### 13.1 颜色方案

|元素|颜色|
|---|---|
|error|红色加粗|
|warning|黄色加粗|
|note|蓝色|
|help|绿色|
|行号|青色|
|代码|白色|
|高亮|红色/黄色下划线|

### 13.2 Rust 实现

Rust

```
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

impl CompileError {
    pub fn print(&self, source: &str, writer: &mut StandardStream) {
        // error[E001]: 
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true)).unwrap();
        write!(writer, "error").unwrap();
        writer.reset().unwrap();
        
        write!(writer, "[{}]: ", self.code).unwrap();
        
        // 消息
        writer.set_color(ColorSpec::new().set_bold(true)).unwrap();
        writeln!(writer, "{}", self.message).unwrap();
        writer.reset().unwrap();
        
        // --> file:line:col
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Cyan))).unwrap();
        writeln!(writer, "  --> {}:{}:{}", self.file, self.line, self.col).unwrap();
        writer.reset().unwrap();
        
        // 源码行
        writeln!(writer, "   |").unwrap();
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Cyan))).unwrap();
        write!(writer, "{:3}", self.line).unwrap();
        writer.reset().unwrap();
        writeln!(writer, " | {}", self.source_line(source)).unwrap();
        
        // 错误指示
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true)).unwrap();
        writeln!(writer, "   | {}^", " ".repeat(self.col - 1)).unwrap();
        writer.reset().unwrap();
        
        // help
        if let Some(help) = &self.help {
            writer.set_color(ColorSpec::new().set_fg(Some(Color::Green))).unwrap();
            write!(writer, "   = help: ").unwrap();
            writer.reset().unwrap();
            writeln!(writer, "{}", help).unwrap();
        }
    }
}
```

---

## 变更记录

|版本|变更|
|---|---|
|v1.1|重构错误告警机制|
||新增 Severity 级别: Error, Warning, Info|
||新增 Shadowing DiagnosticKind (W0002)|
||错误代码按类别重新组织 (0xxx-5xxx)|
||遮蔽从 Error 降级为 Warning|
|v1.0|初始错误信息规范|
||词法错误 L001-L007|
||语法错误 P001-P062|
||名称解析错误 R001-R021|
||类型错误 T001-T051|
||运行时错误 E001-E040|
||警告 W001-W020|
||相似名称建议算法|
||JSON 输出格式|
||颜色输出规范|
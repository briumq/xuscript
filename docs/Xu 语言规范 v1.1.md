# Xu 语言规范 v1.1

---

## 1. 定位与目标

Xu 是一门**强类型脚本语言**，设计目标：

|目标|说明|
|---|---|
|可读无歧义|看到一行就知道在做什么，最多回看两行|
|无空值|没有 `null`，用 Option/Result 表达不确定性|
|自然简洁|关键字接近自然语言，运算符尽量简单|

---

## 2. 关键字

共 23 个关键字，按用途分类：

| 分类  | 关键字                                                              |
| --- | ---------------------------------------------------------------- |
| 控制流 | `if` `else` `while` `for` `in` `break` `continue` `match` `when` |
| 定义  | `let` `var` `func` `return` `has` `with` `does`                  |
| 修饰  | `inner` `static`                                                 |
| 字面  | `self` `true` `false`                                            |
| 模块  | `use` `as`                                                       |

> 预留关键字：`is` `can` `async` `await`（不可作为标识符使用）

---

## 3. 符号与语法

### 3.1 运算符

|类别|符号|
|---|---|
|赋值|`=` `+=` `-=` `*=` `/=`|
|比较|`>` `<` `>=` `<=` `==` `!=`|
|算术|`+` `-` `*` `/` `%`|
|逻辑|`!` `&&` `\|\|`|

> 逻辑运算符 `&&` 和 `||` 支持短路求值。

**运算符优先级**（从高到低）：

|优先级|运算符|结合性|
|---|---|---|
|1|`()` `[]` `.` `#`|左到右|
|2|`!` `-`（一元）|右到左|
|3|`*` `/` `%`|左到右|
|4|`+` `-`|左到右|
|5|`..` `..=`|左到右|
|6|`>` `<` `>=` `<=`|左到右|
|7|`==` `!=`|左到右|
|8|`&&`|左到右|
|9|`\|\|`|左到右|
|10|`=` `+=` `-=` `*=` `/=`|右到左|

### 3.2 结构符号

|符号|语义|用途|
|---|---|---|
|`{ }`|并存/聚合|代码块、结构体定义与字面量、字典|
|`[ ]`|多选一/序列|列表、枚举定义、索引|
|`( )`|分组/调用|表达式分组、函数调用、元组|
|`#`|枚举变体|枚举变体 `Status#pending`|
|`::`|静态方法|静态方法 `User::create()`|
|`.`|实例访问|属性/方法访问、模块成员访问|
|`..` `..=`|范围|整数范围（不含/含结束值）|
|`->`|指向|函数返回类型、匿名函数体|
|`...`|展开|结构体字段展开|
|`:`|标注|类型标注、键值对、单语句块引导|
|`_`|通配|模式匹配占位符|

### 3.3 注释

```xu
// 单行注释

/*
   多行注释
*/
```

### 3.4 分号规则

分号可选。换行等价语句结束，**除以下情况自动续行**：

- 行末是 `. , + - * / = && || ( [ {`
- 括号未闭合
- 下一行以 `. ) ] }` 或二元运算符开头

---

## 4. 变量与作用域

### 4.1 声明方式

|关键字|语义|示例|
|---|---|---|
|`let`|不可重新赋值|`let name = "Tom"`|
|`var`|可重新赋值|`var count = 0`|

**元组解构与通配符**：

```xu
let (x, y) = (1, 2)
let (a, _) = (10, 20)  // 使用 _ 忽略不需要的值
```

```xu
let name = "Tom"
var count = 0

count = count + 1    // ✅
name = "Bob"         // ❌ 不可重新赋值
```

> **注意**：绑定不可变 ≠ 对象不可变。`let` 绑定的列表仍可修改内容：
>
> ```xu
> let list = [1, 2]
> list.add(3)          // ✅ 修改列表内容
> ```

### 4.2 赋值规则

- `x = expr` 只能更新已声明的 `var` 变量
- 对未声明变量赋值 → 编译错误
- 对 `let` 变量赋值 → 编译错误

```xu
x = 1      // ❌ 未声明
let y = 1
y += 1     // ❌ 不可变绑定
```

### 4.3 禁止遮蔽

内层作用域不得声明与外层同名变量：

```xu
var x = 1
if cond {
    let x = 2     // ❌ 禁止遮蔽
}

func f(x: int) {
    var x = 2     // ❌ 禁止遮蔽参数
}
```

---

## 5. 类型系统

### 5.1 基本类型

|类型|说明|示例|
|---|---|---|
|`int`|整数|`42` `0xFF` `0b1010` `1_000_000`|
|`float`|浮点数|`3.14` `1.0e-10`|
|`string`|字符串|`"hello"` `"""多行"""` `r"原始串"`|
|`bool`|布尔值|`true` `false`|

**字符串插值**（支持任意表达式）：

```xu
"Name: {user.name}"
"Sum: {a + b}"
"Result: {if ok { "成功" } else { "失败" }}"
```

### 5.2 集合类型

|类型|非空示例|空集合示例|
|---|---|---|
|列表|`[1, 2, 3]`|`let xs: [int] = []`|
|字典|`{"a": 1}`|`let m: {string: int} = {}`|
|元组|`(1, "hi")`|—|
|范围|`0..5` `0..=5`|—|

> 范围仅支持整数类型。

### 5.3 特殊类型

|类型|说明|
|---|---|
|`void`|无值类型，表示函数无返回值或空状态|
|`any`|动态类型，可持有任意值，运行时类型检查|

> `any` 类型主要用于与外部数据交互（如 JSON 解析），应谨慎使用。

### 5.4 结构体 `has`

```xu
User has {
    name: string
    age: int = 0

    func greet() {
        println("Hi, {self.name}")
    }

    static func create(name: string) -> User {
        return User{ name: name }
    }
}
```

**字面量与展开**：

```xu
let u = User{ name: "Tom", age: 20 }
let older = User{ ...u, age: 21 }    // 浅复制 + 覆盖
```

### 5.5 扩展方法 `does`

为已定义的结构体/枚举添加方法：

```xu
User does {
    func to_json() -> string { ... }
}

User does {
    func validate() -> bool { ... }
}
```

**does 规则**：

|规则|说明|
|---|---|
|作用范围|只能扩展本模块定义的类型|
|多块支持|同一类型可有多个 does 块|
|内置类型|不能扩展 int/string/bool 等内置类型|

### 5.6 枚举 `with`

```xu
// 简单枚举
Status with [ pending | approved | rejected ]

// 带数据的枚举
Response with [
    success(data: string) |
    error(code: int, msg: string)
]

// 使用
let s = Status#pending
let r = Response#error(404, "not found")
```

### 5.7 Option 与 Result

内置泛型类型，用于表达不确定性：

```xu
Option[T] with [ some(value: T) | none ]
Result[T, E] with [ ok(value: T) | err(error: E) ]
```

常见返回类型：

```xu
users.first                      // Option[User]
users.find(func(u) -> u.active)  // Option[User]
map.get("key")                   // Option[V]
file.read("config.json")         // Result[string, IOError]
```

### 5.8 类型推断

**可省略类型**：

```xu
let x = 1                // int
let list = [1, 2, 3]     // [int]
let map = {"a": 1}       // {string: int}
```

**必须标注**：

```xu
let list: [int] = []
let m: {string: int} = {}
func add(a: int, b: int) -> int { ... }
```

---

## 6. 函数

### 6.1 命名函数

```xu
// 基本函数
func add(a: int, b: int) -> int {
    return a + b
}

// 默认参数
func greet(name: string, msg = "Hello") {
    println("{msg}, {name}!")
}

// 多返回值
func div(a: int, b: int) -> (int, int) {
    return (a / b, a % b)
}

// 调用多返回值函数
let (q, r) = div(10, 3)
let (only_q, _) = div(10, 3) // 忽略第二个返回值
```

### 6.2 匿名函数

统一使用 `func` 关键字：

**单表达式形式**：

```xu
let inc = func(x: int) -> x + 1
let add_fn = func(a, b) -> a + b

users
    .filter(func(u) -> u.active)
    .map(func(u) -> u.name)
```

**块形式**：

```xu
let process = func(x: int) -> int {
    let y = x * 2
    if y > 10 { return y }
    return 10
}
```

---

## 7. 控制流

### 7.1 if 语句与表达式

**语句形式**（else 可省略）：

```xu
if x > 0 {
    println("positive")
} else if x < 0 {
    println("negative")
} else {
    println("zero")
}
```

**单语句简写**（使用 `:` 引导一条语句作为分支体）：

```xu
if x > 0: println("positive")
if x < 0:
    println("negative")
```

**表达式形式**（必须有 else，类型一致）：

```xu
let sign = if x >= 0: 1 else: -1
```

### 7.2 循环

```xu
// while
var i = 0
while i < 10: i += 1

// for-in
for i in 0..5: println(i)

for (key, value) in map: println("{key}: {value}")
```

**单语句简写**：

```xu
while i < 10: i += 1
for i in 0..5: println(i)
```

### 7.3 match 模式匹配

用于穷尽所有情况的模式匹配。

**语句形式**：

```xu
match status {
    Status#pending:  println("待处理")
    Status#approved: println("已通过")
    Status#rejected: println("已拒绝")
    _: println("未知状态")
}
```

**表达式形式**：

```xu
let text = match status {
    Status#pending:  "待处理"
    Status#approved: "已通过"
    Status#rejected: "已拒绝"
    _: "未知"
}
```

**匹配 Result**：

```xu
match fetch_users("/api/users") {
    Result#ok(data): for user in data: println(user.name)
    Result#err(e): println("错误: {e}")
    _: println("未知结果")
}
```

> `match` 语句必须提供一个最终的 `_ { ... }` 默认分支（用于兜底/非穷尽匹配）。

---

## 8. when 条件绑定

`when` 专门用于 Option/Result 解包，强调"快乐路径优先"，**仅作为语句使用**。

### 8.1 基本用法

```xu
when user = find_user(id): println(user.name)
else: println("未找到用户")
```

### 8.2 多绑定短路

任一绑定失败则跳转到 else：

```xu
when a = get_a(), b = get_b(), c = get_c(): use(a, b, c)
else: println("信息不完整")
```

### 8.3 when vs match

|特性|when|match|
|---|---|---|
|定位|Option/Result 解包语法糖|通用模式匹配|
|分支|成功/失败 二选一|穷尽所有变体|
|多绑定|✅ 原生支持|❌ 需嵌套|
|作为表达式|❌|✅|

### 8.4 单语句简写（冒号语法）

Xu 支持使用 `:` 引导单条语句，作为 `{ }` 块的语法糖。以下是所有支持冒号语法的语句和表达式：

|语句/表达式|示例|
|---|---|
|if 语句|`if x > 0: println("positive")`|
|else if 分支|`else if x < 0: println("negative")`|
|else 分支|`else: println("zero")`|
|while 语句|`while i < 10: i = i + 1`|
|for 语句|`for i in 0..5: sum = sum + i`|
|match 语句 arm|`1: result = "one"`|
|match 表达式 arm|`1: "one"`|
|when 成功分支|`when val = get(): use(val)`|
|when else 分支|`else: handle_error()`|
|if 表达式 then|`if true: 100 else: 200`|
|if 表达式 else|`if cond: a else: b`|

> 注意：函数定义、结构体定义、枚举定义等不支持冒号语法，必须使用 `{ }` 块。

---

## 9. 错误处理模式

Xu 没有异常机制，使用 Result 类型显式处理错误。以下是三种推荐模式：

### 9.1 when 多绑定

适用于只关心成功/失败，不需要区分具体错误：

```xu
func load_config() -> Result[Config, string] {
    when content = file.read("config.json"), config = parse(content): return Result#ok(config)
    else: return Result#err("配置加载失败")
}
```

|优点|缺点|
|---|---|
|简洁|丢失具体错误信息|

### 9.2 match 嵌套

适用于需要精确处理每一步的错误：

```xu
func load_config() -> Result[Config, string] {
    match file.read("config.json") {
        Result#ok(content) {
            match parse(content) {
                Result#ok(config): return Result#ok(config)
                Result#err(e): return Result#err("解析失败: {e}")
                _: return Result#err("未知解析结果")
            }
        }
        Result#err(e): return Result#err("读取失败: {e}")
        _: return Result#err("未知读取结果")
    }
}
```

|优点|缺点|
|---|---|
|错误信息精确|嵌套较深|

### 9.3 组合子链式

适用于错误可以统一处理的场景：

```xu
func load_config() -> Result[Config, string] {
    return file.read("config.json")
        .then(func(s) -> parse(s))
        .map_err(func(e) -> "配置加载失败: {e}")
}
```

|优点|缺点|
|---|---|
|链式简洁|各步错误信息被合并|

### 9.4 模式选择

|场景|推荐模式|
|---|---|
|只关心成功/失败|when 多绑定|
|需要区分每步错误|match 嵌套|
|错误可统一处理|组合子链式|

---

## 10. 无空设计

### 10.1 核心约束

- 没有 `null` 关键字
- 所有变量和字段必须初始化
- 用 Option/Result 显式表达不确定性

### 10.2 Option 组合子

|方法|说明|
|---|---|
|`.has` `.none`|是否有值/无值（属性）|
|`.or(v)`|有值取值，否则用默认值|
|`.or_else(func)`|惰性默认值|
|`.map(func)`|映射变换|
|`.then(func)`|链式操作（返回 Option）|
|`.each(func)`|有值则执行|
|`.filter(pred)`|不满足则变为 none|

### 10.3 Result 组合子

同 Option，额外有：

|方法|说明|
|---|---|
|`.map_err(func)`|转换错误类型/信息|

```xu
let config = file.read("config.json")
    .then(func(s) -> parse(s))
    .map_err(func(e) -> "配置加载失败: {e}")
    .or(default_config)
```

### 10.4 字符串方法

|方法|说明|示例|
|---|---|---|
|`.length()`|获取字符串长度|`"hello".length() // 返回 5`|
|`.starts_with(prefix)`|检查是否以指定前缀开头|`"hello".starts_with("he") // 返回 true`|
|`.ends_with(suffix)`|检查是否以指定后缀结尾|`"hello".ends_with("lo") // 返回 true`|
|`.split(separator)`|按分隔符分割字符串|`"a,b,c".split(",") // 返回 ["a", "b", "c"]`|
|`.trim()`|去除首尾空白|`"  hello  ".trim() // 返回 "hello"`|
|`.to_upper()`|转换为大写|`"hello".to_upper() // 返回 "HELLO"`|
|`.to_lower()`|转换为小写|`"HELLO".to_lower() // 返回 "hello"`|

### 10.5 整数方法

|方法|说明|示例|
|---|---|---|
|`.to_string()`|将整数转换为字符串|`42.to_string() // 返回 "42"`|
|`.abs()`|获取绝对值|`(-10).abs() // 返回 10`|
|`.to_base(base)`|转换为指定进制|`255.to_base(16) // 返回 "FF"`|
|`.is_even()`|检查是否为偶数|`42.is_even() // 返回 true`|
|`.is_odd()`|检查是否为奇数|`43.is_odd() // 返回 true`|

### 10.6 浮点数方法

|方法|说明|示例|
|---|---|---|
|`.to_string()`|将浮点数转换为字符串|`3.14.to_string() // 返回 "3.14"`|
|`.to_int()`|将浮点数转换为整数（截断小数部分）|`3.99.to_int() // 返回 3`|
|`.abs()`|获取绝对值|`(-3.14).abs() // 返回 3.14`|
|`.round()`|四舍五入|`3.14.round() // 返回 3`|
|`.floor()`|向下取整|`3.99.floor() // 返回 3`|
|`.ceil()`|向上取整|`3.01.ceil() // 返回 4`|

### 10.7 布尔值方法

|方法|说明|示例|
|---|---|---|
|`.to_string()`|将布尔值转换为字符串|`true.to_string() // 返回 "true"`|
|`.not()`|逻辑非操作|`true.not() // 返回 false`|

### 10.8 列表方法

|方法|说明|示例|
|---|---|---|
|`.length()`|获取列表长度|`[1, 2, 3].length() // 返回 3`|
|`.push(item)`|向列表追加元素|`let list = [1, 2]; list.push(3); // 现在 list 为 [1, 2, 3]`|
|`.pop()`|移除并返回最后一个元素|`let list = [1, 2, 3]; list.pop(); // 返回 3，list 变为 [1, 2]`|
|`.reverse()`|反转列表|`let list = [1, 2, 3]; list.reverse(); // 现在 list 为 [3, 2, 1]`|
|`.join(separator)`|用分隔符连接列表元素|`["a", "b", "c"].join(",") // 返回 "a,b,c"`|
|`.contains(item)`|检查列表是否包含指定元素|`[1, 2, 3].contains(2) // 返回 true`|
|`.clear()`|清空列表|`let list = [1, 2, 3]; list.clear(); // 现在 list 为 []`|
|`.remove(index)`|按索引删除元素并返回|`let list = [1, 2, 3]; list.remove(0); // 返回 1，list 变为 [2, 3]`|

### 10.9 字典方法

|方法|说明|示例|
|---|---|---|
|`.length()`|获取字典长度|`{"a": 1, "b": 2}.length() // 返回 2`|
|`.insert(key, value)`|插入键值对|`let dict = {"a": 1}; dict.insert("b", 2); // 现在 dict 为 {"a": 1, "b": 2}`|
|`.get(key)`|获取指定键的值|`{"a": 1}.get("a") // 返回 Option.some(1)`|
|`.keys()`|获取所有键|`{"a": 1, "b": 2}.keys() // 返回 ["a", "b"]`|
|`.values()`|获取所有值|`{"a": 1, "b": 2}.values() // 返回 [1, 2]`|
|`.merge(other)`|合并另一个字典|`{"a": 1}.merge({"b": 2}) // 返回 {"a": 1, "b": 2}`|

---

## 11. 容器访问

|方式|语法|缺失时行为|
|---|---|---|
|强访问|`list[0]` `map["key"]`|panic|
|安全访问|`list.first` `list.get(0)` `map.get("key")`|返回 Option|

> panic 为不可恢复错误，终止脚本执行。

---

## 12. 模块与可见性

### 12.1 模块导入

```xu
use "utils"
use "utils" as u
```

- 每个文件是一个模块
- `use` 时执行模块顶层一次并缓存
- `use "path"` 会将模块绑定到一个默认别名（由路径末尾推断，例如 `utils`），不会把导出成员注入当前作用域
- 访问导出成员使用 `alias.member`；`as` 可显式指定别名

### 12.2 可见性

默认情况下，所有顶层定义（函数、变量、结构体、枚举）和扩展方法都是**公开**的（模块外可见）。
使用 `inner` 关键字可将其限制为**仅本文件可见**（模块私有）。

|修饰符|说明|适用范围|
|---|---|---|
|（默认）|公开|顶层定义、方法|
|`inner`|仅本文件可见|顶层定义、方法|

```xu
// 仅本模块可见的变量
inner var counter = 0

// 仅本模块可见的结构体
inner Foo has { x: int }

User does {
    // 仅本模块可见的方法（即私有辅助方法）
    inner func internal_helper() {}

    // 公开方法
    func public_api() {}
}
```

---

## 13. 完整示例

```xu
use "http" as http

// 枚举定义
Status with [ pending | approved | rejected ]

// 结构体定义
User has {
    name: string
    age: int = 0
    status: Status = Status#pending

    func greet(): println("Hello, {self.name}!")

    func is_adult() -> bool: return self.age >= 18

    static func create(name: string, age: int) -> User: return User{ name: name, age: age }
}

// 扩展方法
User does {
    func to_json() -> string: return """{"name": "{self.name}", "age": {self.age}}"""
}

// 组合子链式处理错误
func fetch_users(url: string) -> Result[[User], string] {
    return http.get(url)
        .then(func(resp) -> parse_users(resp.body))
        .map_err(func(e) -> "请求失败: {e}")
}

// 主函数
func main() {
    let users = [
        User::create("Alice", 20),
        User::create("Bob", 16)
    ]

    // 链式过滤
    let adults = users.filter(func(u) -> u.is_adult())
    if adults.any: println("成年人数: {adults.length}")

    // when 条件绑定
    when user = users.find(func(u) -> u.name == "Alice"): user.greet()
    else: println("未找到 Alice")

    // 多绑定短路
    when first = users.first, second = users.get(1): println("{first.name} 和 {second.name}")
    else: println("用户不足")

    // 结构体展开
    when first = users.first {
        let older = User{ ...first, age: first.age + 1 }
        println("{older.name} 明年 {older.age} 岁")
    } else: println("没有用户")

    // match 表达式
    let status_text = match users.first.or(User{ name: "?" }).status {
        Status#pending:  "待审核"
        Status#approved: "已通过"
        Status#rejected: "已拒绝"
        _: "未知"
    }
    println("状态: {status_text}")

    // match 处理 Result
    match fetch_users("/api/users") {
        Result#ok(data): for user in data: println(user.name)
        Result#err(msg): println("错误: {msg}")
        _: println("未知结果")
    }
}
```

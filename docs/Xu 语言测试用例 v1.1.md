# Xu 语言测试用例 v1.1

---

## 1. 字面量与基本类型

### 1.1 整数

xu

```
// test: integer_literals
let a = 42
let b = -17
let c = 0xFF
let d = 0b1010
let e = 1_000_000

assert(a is 42)
assert(b is -17)
assert(c is 255)
assert(d is 10)
assert(e is 1000000)
```

### 1.2 浮点数

xu

```
// test: float_literals
let a = 3.14
let b = -0.5
let c = 1.0e10
let d = 2.5e-3

assert(a is 3.14)
assert(b is -0.5)
assert(c is 10000000000.0)
assert(d is 0.0025)
```

### 1.3 字符串

xu

```
// test: string_literals
let a = "hello"
let b = "line1\nline2"
let c = """
多行
字符串
"""
let d = r"raw\nstring"

assert(a is "hello")
assert(b.contains("\n"))
assert(c.contains("多行"))
assert(d.contains("\\n"))
```

### 1.4 布尔值

xu

```
// test: boolean_literals
let t = true
let f = false

assert(t is true)
assert(f is false)
assert(t isnt f)
```

---

## 2. 变量与作用域

### 2.1 let 不可变绑定

xu

```
// test: let_immutable
let x = 10
assert(x is 10)

// test: let_reassign_error [expect_error: "cannot assign to immutable"]
let x = 10
x = 20
```

### 2.2 var 可变绑定

xu

```
// test: var_mutable
var x = 10
assert(x is 10)
x = 20
assert(x is 20)
x += 5
assert(x is 25)
```

### 2.3 未声明变量

xu

```
// test: undefined_variable_error [expect_error: "undefined"]
x = 10
```

### 2.4 禁止遮蔽

xu

```
// test: shadowing_block_error [expect_error: "shadowing"]
let x = 1
if true {
    let x = 2
}

// test: shadowing_function_error [expect_error: "shadowing"]
func f(x: int) {
    let x = 2
}

// test: shadowing_for_error [expect_error: "shadowing"]
let i = 0
for i in 0..5 {
    println(i)
}
```

### 2.5 let 绑定对象可修改

xu

```
// test: let_object_mutation
let list = [1, 2, 3]
list.add(4)
assert(list.length is 4)
assert(list[3] is 4)
```

---

## 3. 运算符

### 3.1 算术运算

xu

```
// test: arithmetic_int
assert(10 + 3 is 13)
assert(10 - 3 is 7)
assert(10 * 3 is 30)
assert(10 / 3 is 3)
assert(10 % 3 is 1)

// test: arithmetic_float
assert(10.0 + 3.0 is 13.0)
assert(10.0 / 4.0 is 2.5)

// test: division_by_zero [expect_panic: "division by zero"]
let x = 10 / 0
```

### 3.2 比较运算

xu

```
// test: comparison
assert(5 > 3)
assert(3 < 5)
assert(5 >= 5)
assert(5 <= 5)
assert(5 is 5)
assert(5 == 5)
assert(5 isnt 3)
assert(5 != 3)
```

### 3.3 逻辑运算

xu

```
// test: logical_and
assert(true and true)
assert(not (true and false))
assert(not (false and true))
assert(not (false and false))

// test: logical_or
assert(true or true)
assert(true or false)
assert(false or true)
assert(not (false or false))

// test: logical_not
assert(not false)
assert(not (not true))

// test: short_circuit_and
var called = false
func side_effect() -> bool {
    called = true
    return true
}
let result = false and side_effect()
assert(not called)

// test: short_circuit_or
var called = false
func side_effect() -> bool {
    called = true
    return true
}
let result = true or side_effect()
assert(not called)
```

### 3.4 字符串连接

xu

```
// test: string_concat
let a = "hello"
let b = " world"
assert(a + b is "hello world")
```

---

## 4. 字符串插值

xu

```
// test: string_interpolation_simple
let name = "Alice"
let msg = "Hello, {name}!"
assert(msg is "Hello, Alice!")

// test: string_interpolation_expression
let a = 10
let b = 20
let msg = "Sum: {a + b}"
assert(msg is "Sum: 30")

// test: string_interpolation_nested
let ok = true
let msg = "Status: {if ok { "成功" } else { "失败" }}"
assert(msg is "Status: 成功")
```

---

## 5. 集合类型

### 5.1 列表

xu

```
// test: list_literal
let list = [1, 2, 3]
assert(list.length is 3)
assert(list[0] is 1)
assert(list[1] is 2)
assert(list[2] is 3)

// test: list_empty
let list: [int] = []
assert(list.length is 0)

// test: list_add
let list = [1, 2]
list.add(3)
assert(list.length is 3)
assert(list[2] is 3)

// test: list_index_out_of_bounds [expect_panic: "index out of bounds"]
let list = [1, 2, 3]
let x = list[10]

// test: list_safe_access
let list = [1, 2, 3]
assert(list.first.has)
assert(list.first.or(0) is 1)
assert(list.get(10).none)
```

### 5.2 字典

xu

```
// test: dict_literal
let dict = {"a": 1, "b": 2}
assert(dict["a"] is 1)
assert(dict["b"] is 2)

// test: dict_empty
let dict: {string: int} = {}
assert(dict.get("a").none)

// test: dict_key_not_found [expect_panic: "key not found"]
let dict = {"a": 1}
let x = dict["b"]

// test: dict_safe_access
let dict = {"a": 1}
assert(dict.get("a").has)
assert(dict.get("a").or(0) is 1)
assert(dict.get("b").none)
```

### 5.3 集合

xu

```
// test: set_literal
let s = set{1, 2, 3, 2, 1}
assert(s.length is 3)
assert(s.contains(1))
assert(s.contains(2))
assert(s.contains(3))

// test: set_empty
let s: {int} = set{}
assert(s.length is 0)
```

### 5.4 元组

xu

```
// test: tuple_literal
let t = (1, "hello", true)
assert(t.0 is 1)
assert(t.1 is "hello")
assert(t.2 is true)

// test: tuple_destructure
let (a, b, c) = (1, "hello", true)
assert(a is 1)
assert(b is "hello")
assert(c is true)
```

### 5.5 范围

xu

```
// test: range_exclusive
var sum = 0
for i in 0..5 {
    sum += i
}
assert(sum is 10)  // 0+1+2+3+4

// test: range_inclusive
var sum = 0
for i in 0..=5 {
    sum += i
}
assert(sum is 15)  // 0+1+2+3+4+5
```

---

## 6. 函数

### 6.1 基本函数

xu

```
// test: function_basic
func add(a: int, b: int) -> int {
    return a + b
}
assert(add(2, 3) is 5)

// test: function_no_return
func greet(name: string) {
    println("Hello, {name}!")
}
greet("Alice")
```

### 6.2 默认参数

xu

```
// test: function_default_param
func greet(name: string, msg = "Hello") -> string {
    return "{msg}, {name}!"
}
assert(greet("Alice") is "Hello, Alice!")
assert(greet("Bob", "Hi") is "Hi, Bob!")
```

### 6.3 多返回值

xu

```
// test: function_multiple_return
func divmod(a: int, b: int) -> (int, int) {
    return (a / b, a % b)
}
let (q, r) = divmod(10, 3)
assert(q is 3)
assert(r is 1)
```

### 6.4 匿名函数（单表达式）

xu

```
// test: lambda_simple
let inc = |x: int| x + 1
assert(inc(5) is 6)

// test: lambda_inferred
let add = |a, b| a + b
assert(add(2, 3) is 5)
```

### 6.5 匿名函数（块形式）

xu

```
// test: lambda_block
let process = |x: int| -> int {
    let y = x * 2
    if y > 10 {
        return y
    }
    return 10
}
assert(process(3) is 10)
assert(process(10) is 20)
```

### 6.6 闭包

xu

```
// test: closure
func make_counter() -> || -> int {
    var count = 0
    return || {
        count += 1
        return count
    }
}
let counter = make_counter()
assert(counter() is 1)
assert(counter() is 2)
assert(counter() is 3)
```

---

## 7. 控制流

### 7.1 if 语句

xu

```
// test: if_statement
var result = ""
let x = 5
if x > 0 {
    result = "positive"
} else if x < 0 {
    result = "negative"
} else {
    result = "zero"
}
assert(result is "positive")

// test: if_no_else
var result = "default"
if false {
    result = "changed"
}
assert(result is "default")

// test: if_single_stmt_colon
var y = 0
if true: y = 1
assert(y is 1)
```

### 7.2 if 表达式

xu

```
// test: if_expression
let x = 5
let sign = if x >= 0 { 1 } else { -1 }
assert(sign is 1)

// test: if_expression_negative
let x = -5
let sign = if x >= 0 { 1 } else { -1 }
assert(sign is -1)
```

### 7.3 while 循环

xu

```
// test: while_loop
var i = 0
var sum = 0
while i < 5 {
    sum += i
    i += 1
}
assert(sum is 10)

// test: while_break
var i = 0
while true {
    i += 1
    if i is 5 {
        break
    }
}
assert(i is 5)

// test: while_continue
var sum = 0
var i = 0
while i < 10 {
    i += 1
    if i % 2 is 0 {
        continue
    }
    sum += i
}
assert(sum is 25)  // 1+3+5+7+9
```

### 7.4 for 循环

xu

```
// test: for_range
var sum = 0
for i in 0..5 {
    sum += i
}
assert(sum is 10)

// test: for_list
let list = [1, 2, 3, 4, 5]
var sum = 0
for x in list {
    sum += x
}
assert(sum is 15)

// test: for_dict
let dict = {"a": 1, "b": 2, "c": 3}
var sum = 0
for (key, value) in dict {
    sum += value
}
assert(sum is 6)

// test: for_break
var last = 0
for i in 0..10 {
    if i is 5 {
        break
    }
    last = i
}
assert(last is 4)
```

### 7.5 match 语句

xu

```
// test: match_enum
Status with [ pending | approved | rejected ]

let s = Status#approved
var result = ""
match s {
    Status#pending  { result = "待处理" }
    Status#approved { result = "已通过" }
    Status#rejected { result = "已拒绝" }
    _ { assert(false) }
}
assert(result is "已通过")
```

### 7.6 match 表达式

xu

```
// test: match_expression
Status with [ pending | approved | rejected ]

let s = Status#rejected
let text = match s {
    Status#pending  { "待处理" }
    Status#approved { "已通过" }
    Status#rejected { "已拒绝" }
    _ { "未知" }
}
assert(text is "已拒绝")
```

### 7.7 match 带数据枚举

xu

```
// test: match_enum_data
Response with [
    success(data: string) |
    error(code: int, msg: string)
]

let r = Response#error(404, "not found")
let result = match r {
    Response#success(data) { "OK: {data}" }
    Response#error(code, msg) { "Error {code}: {msg}" }
    _ { "unknown" }
}
assert(result is "Error 404: not found")
```

### 7.8 match 默认分支（_）

xu

```
// test: match_else
let x = 42
let result = match x {
    0 { "zero" }
    1 { "one" }
    _ { "other" }
}
assert(result is "other")
```

### 7.9 match 通配符

xu

```
// test: match_wildcard
let t = (1, 2, 3)
let result = match t {
    (0, _, _) { "starts with zero" }
    (_, 0, _) { "middle is zero" }
    _ { "none zero" }
}
assert(result is "none zero")
```

---

## 8. when 条件绑定

### 8.1 Option 绑定

xu

```
// test: when_option_some
let list = [1, 2, 3]
var result = 0
when first = list.first {
    result = first
} else {
    result = -1
}
assert(result is 1)

// test: when_option_none
let list: [int] = []
var result = 0
when first = list.first {
    result = first
} else {
    result = -1
}
assert(result is -1)
```

### 8.2 多绑定短路

xu

```
// test: when_multi_bind_success
let a = Option#some(1)
let b = Option#some(2)
let c = Option#some(3)
var result = 0
when x = a, y = b, z = c {
    result = x + y + z
} else {
    result = -1
}
assert(result is 6)

// test: when_multi_bind_fail
let a = Option#some(1)
let b = Option#none
let c = Option#some(3)
var result = 0
when x = a, y = b, z = c {
    result = x + y + z
} else {
    result = -1
}
assert(result is -1)
```

### 8.3 Result 绑定

xu

```
// test: when_result_ok
let r = Result#ok(42)
var result = 0
when value = r {
    result = value
} else {
    result = -1
}
assert(result is 42)

// test: when_result_err
let r = Result#err("failed")
var result = 0
when value = r {
    result = value
} else {
    result = -1
}
assert(result is -1)
```

### 8.4 when 不能作为表达式

xu

```
// test: when_not_expression [expect_error: "when cannot be used as expression"]
let x = when a = Option#some(1) { a } else { 0 }
```

---

## 9. 结构体

### 9.1 定义与实例化

xu

```
// test: struct_basic
User has {
    name: string
    age: int = 0
}

let u = User{ name: "Alice", age: 20 }
assert(u.name is "Alice")
assert(u.age is 20)

// test: struct_default_field
User has {
    name: string
    age: int = 0
}

let u = User{ name: "Bob" }
assert(u.name is "Bob")
assert(u.age is 0)
```

### 9.2 方法

xu

```
// test: struct_method
User has {
    name: string
    age: int

    func greet() -> string {
        return "Hello, {self.name}!"
    }

    func is_adult() -> bool {
        return self.age >= 18
    }
}

let u = User{ name: "Alice", age: 20 }
assert(u.greet() is "Hello, Alice!")
assert(u.is_adult())
```

### 9.3 静态方法

xu

```
// test: struct_static_method
User has {
    name: string
    age: int

    static func create(name: string, age: int) -> User {
        return User{ name: name, age: age }
    }
}

let u = User.create("Alice", 20)
assert(u.name is "Alice")
assert(u.age is 20)
```

### 9.4 结构体展开

xu

```
// test: struct_spread
User has {
    name: string
    age: int
}

let u1 = User{ name: "Alice", age: 20 }
let u2 = User{ ...u1, age: 21 }
assert(u2.name is "Alice")
assert(u2.age is 21)
assert(u1.age is 20)  // 原对象不变
```

---

## 10. does 扩展

### 10.1 扩展方法

xu

```
// test: does_extend
User has {
    name: string
    age: int
}

User does {
    func to_json() -> string {
        return """{"name": "{self.name}", "age": {self.age}}"""
    }
}

let u = User{ name: "Alice", age: 20 }
assert(u.to_json() is """{"name": "Alice", "age": 20}""")
```

### 10.2 多个 does 块

xu

```
// test: does_multiple_blocks
User has {
    name: string
}

User does {
    func method1() -> string { return "m1" }
}

User does {
    func method2() -> string { return "m2" }
}

let u = User{ name: "Test" }
assert(u.method1() is "m1")
assert(u.method2() is "m2")
```

### 10.3 禁止扩展内置类型

xu

```
// test: does_builtin_error [expect_error: "cannot extend builtin type"]
int does {
    func is_even() -> bool { return self % 2 is 0 }
}
```

---

## 11. 枚举

### 11.1 简单枚举

xu

```
// test: enum_simple
Status with [ pending | approved | rejected ]

let s = Status#pending
assert(s is Status#pending)
assert(s isnt Status#approved)
```

### 11.2 带数据枚举

xu

```
// test: enum_with_data
Response with [
    success(data: string) |
    error(code: int, msg: string)
]

let r1 = Response#success("ok")
let r2 = Response#error(404, "not found")

match r1 {
    Response#success(data) { assert(data is "ok") }
    Response#error(_, _) { assert(false) }
    _ { assert(false) }
}

match r2 {
    Response#success(_) { assert(false) }
    Response#error(code, msg) {
        assert(code is 404)
        assert(msg is "not found")
    }
    _ { assert(false) }
}
```

### 11.3 枚举扩展方法（does）

xu

```
// test: enum_does_method
Color with [ red | blue ]

Color does {
    func is_red() -> bool {
        return self is Color#red
    }
}

let c = Color#red
assert(c.is_red())
```

---

## 12. Option

### 12.1 构造

xu

```
// test: option_some
let opt = Option#some(42)
assert(opt.has)
assert(not opt.none)

// test: option_none
let opt: Option[int] = Option#none
assert(opt.none)
assert(not opt.has)
```

### 12.2 组合子

xu

```
// test: option_or
let a = Option#some(1)
let b: Option[int] = Option#none
assert(a.or(0) is 1)
assert(b.or(0) is 0)

// test: option_or_else
let a: Option[int] = Option#none
let result = a.or_else(|| 42)
assert(result is 42)

// test: option_map
let a = Option#some(5)
let b = a.map(|x| x * 2)
assert(b.or(0) is 10)

let c: Option[int] = Option#none
let d = c.map(|x| x * 2)
assert(d.none)

// test: option_then
let a = Option#some(5)
let b = a.then(|x| if x > 0 { Option#some(x * 2) } else { Option#none })
assert(b.or(0) is 10)

// test: option_filter
let a = Option#some(5)
let b = a.filter(|x| x > 3)
let c = a.filter(|x| x > 10)
assert(b.has)
assert(c.none)

// test: option_each
let a = Option#some(5)
var result = 0
a.each(|x| { result = x })
assert(result is 5)

let b: Option[int] = Option#none
result = 0
b.each(|x| { result = x })
assert(result is 0)
```

---

## 13. Result

### 13.1 构造

xu

```
// test: result_ok
let r = Result#ok(42)
match r {
    Result#ok(v) { assert(v is 42) }
    Result#err(_) { assert(false) }
    _ { assert(false) }
}

// test: result_err
let r = Result#err("failed")
match r {
    Result#ok(_) { assert(false) }
    Result#err(e) { assert(e is "failed") }
    _ { assert(false) }
}
```

### 13.2 组合子

xu

```
// test: result_or
let a = Result#ok(42)
let b = Result#err("failed")
assert(a.or(0) is 42)
assert(b.or(0) is 0)

// test: result_map
let a = Result#ok(5)
let b = a.map(|x| x * 2)
assert(b.or(0) is 10)

// test: result_map_err
let a = Result#err("error")
let b = a.map_err(|e| "wrapped: {e}")
match b {
    Result#ok(_) { assert(false) }
    Result#err(e) { assert(e is "wrapped: error") }
    _ { assert(false) }
}

// test: result_then
let a = Result#ok(5)
let b = a.then(|x| Result#ok(x * 2))
assert(b.or(0) is 10)

let c = Result#ok(5)
let d = c.then(|x| Result#err("failed"))
assert(d.or(0) is 0)
```

---

## 14. 错误处理模式

### 14.1 when 多绑定模式

xu

```
// test: error_handling_when
func read_file(path: string) -> Result[string, string] {
    if path is "exist.txt" {
        return Result#ok("content")
    }
    return Result#err("file not found")
}

func parse(content: string) -> Result[int, string] {
    if content is "content" {
        return Result#ok(42)
    }
    return Result#err("parse error")
}

var result = 0
when content = read_file("exist.txt"), value = parse(content) {
    result = value
} else {
    result = -1
}
assert(result is 42)

when content = read_file("noexist.txt"), value = parse(content) {
    result = value
} else {
    result = -1
}
assert(result is -1)
```

### 14.2 match 嵌套模式

xu

```
// test: error_handling_match
func read_file(path: string) -> Result[string, string] {
    if path is "exist.txt" {
        return Result#ok("content")
    }
    return Result#err("file not found")
}

func parse(content: string) -> Result[int, string] {
    return Result#ok(42)
}

func load() -> Result[int, string] {
    match read_file("exist.txt") {
        Result#ok(content) {
            match parse(content) {
                Result#ok(value) { return Result#ok(value) }
                Result#err(e) { return Result#err("parse failed: {e}") }
            }
        }
        Result#err(e) { return Result#err("read failed: {e}") }
    }
}

let r = load()
assert(r.or(0) is 42)
```

### 14.3 组合子链式模式

xu

```
// test: error_handling_chain
func read_file(path: string) -> Result[string, string] {
    if path is "exist.txt" {
        return Result#ok("content")
    }
    return Result#err("file not found")
}

func parse(content: string) -> Result[int, string] {
    return Result#ok(42)
}

let result = read_file("exist.txt")
    .then(|s| parse(s))
    .map_err(|e| "failed: {e}")
    .or(-1)

assert(result is 42)
```

---

## 15. 模块与可见性

### 15.1 模块导入

xu

```
// file: math.xu
func add(a: int, b: int) -> int {
    return a + b
}

func multiply(a: int, b: int) -> int {
    return a * b
}

// file: main.xu
// test: module_import
use "math"

assert(math.add(2, 3) is 5)
assert(math.multiply(2, 3) is 6)
```

> `use "math"` 默认会将模块绑定到同名别名 `math`（由路径推断）；也可用 `as` 显式指定别名。

### 15.2 模块别名

xu

```
// file: math.xu
func add(a: int, b: int) -> int {
    return a + b
}

// file: main.xu
// test: module_alias
use "math" as m

assert(m.add(2, 3) is 5)
```

### 15.3 pub 可见性

xu

```
// file: counter.xu
var count = 0

pub func increment() {
    count += 1
}

pub func get() -> int {
    return count
}

// file: main.xu
// test: inner_visibility
use "counter"

counter.increment()
counter.increment()
assert(counter.get() is 2)

// test: inner_access_error [expect_error: "Unknown member: count"]
use "counter"
let x = counter.count
```

---

## 16. 链式操作

xu

```
// test: method_chaining
User has {
    name: string
    age: int
    active: bool

    func is_adult() -> bool {
        return self.age >= 18
    }
}

let users = [
    User{ name: "Alice", age: 20, active: true },
    User{ name: "Bob", age: 16, active: true },
    User{ name: "Charlie", age: 25, active: false },
    User{ name: "Diana", age: 30, active: true }
]

let names = users
    .filter(|u| u.active)
    .filter(|u| u.is_adult())
    .map(|u| u.name)

assert(names.length is 2)
assert(names[0] is "Alice")
assert(names[1] is "Diana")
```

---

## 17. 边界情况

### 17.1 空集合操作

xu

```
// test: empty_list_operations
let list: [int] = []
assert(list.length is 0)
assert(not list.any)
assert(list.first.none)
assert(list.filter(|x| true).length is 0)
assert(list.map(|x| x * 2).length is 0)
```

### 17.2 嵌套结构

xu

```
// test: nested_struct
Address has {
    city: string
    street: string
}

User has {
    name: string
    address: Address
}

let u = User{
    name: "Alice",
    address: Address{ city: "Beijing", street: "Main St" }
}

assert(u.address.city is "Beijing")
```

### 17.3 递归函数

xu

```
// test: recursion
func factorial(n: int) -> int {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

assert(factorial(0) is 1)
assert(factorial(1) is 1)
assert(factorial(5) is 120)

// test: fibonacci
func fib(n: int) -> int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

assert(fib(0) is 0)
assert(fib(1) is 1)
assert(fib(10) is 55)
```

### 17.4 深层嵌套控制流

xu

```
// test: nested_control_flow
var result = 0
for i in 0..3 {
    for j in 0..3 {
        if i is j {
            continue
        }
        if i + j > 3 {
            break
        }
        result += 1
    }
}
assert(result is 4)
```

---

## 18. 综合示例

xu

```
// test: comprehensive_example
Status with [ pending | approved | rejected ]

User has {
    id: int
    name: string
    email: string
    status: Status = Status#pending

    func is_active() -> bool {
        return self.status is Status#approved
    }

    static func create(id: int, name: string, email: string) -> User {
        return User{ id: id, name: name, email: email }
    }
}

User does {
    func to_string() -> string {
        return "User({self.id}, {self.name})"
    }
}

func find_user(users: [User], id: int) -> Option[User] {
    for user in users {
        if user.id is id {
            return Option#some(user)
        }
    }
    return Option#none
}

func approve_user(user: User) -> User {
    return User{ ...user, status: Status#approved }
}

// 主测试
let users = [
    User.create(1, "Alice", "alice@example.com"),
    User.create(2, "Bob", "bob@example.com"),
    User.create(3, "Charlie", "charlie@example.com")
]

// 查找并处理
when user = find_user(users, 2) {
    let approved = approve_user(user)
    assert(approved.is_active())
    assert(approved.name is "Bob")
}

// 链式过滤
let pending_users = users.filter(|u| u.status is Status#pending)
assert(pending_users.length is 3)

// match 表达式
let first_status = match users.first {
    Option#some(u) {
        match u.status {
            Status#pending  { "待处理" }
            Status#approved { "已通过" }
            Status#rejected { "已拒绝" }
            _ { "未知" }
        }
    }
    Option#none { "无用户" }
    _ { "未知" }
}
assert(first_status is "待处理")

// Option 组合子
let first_name = users.first
    .map(|u| u.name)
    .or("匿名")
assert(first_name is "Alice")

// 不存在的用户
let missing = find_user(users, 999)
    .map(|u| u.name)
    .or("未找到")
assert(missing is "未找到")
```

---

## 附录：测试标记说明

|标记|说明|
|---|---|
|`// test: name`|测试用例名称|
|`[expect_error: "msg"]`|期望编译错误包含 msg|
|`[expect_panic: "msg"]`|期望运行时 panic 包含 msg|
|`assert(expr)`|断言表达式为 true|

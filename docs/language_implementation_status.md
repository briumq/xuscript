# 语言实现状态与重构计划 (Language Implementation Status & Refactoring Plan)

## 状态简表 (Status Summary)

| 功能/问题 (Feature/Issue) | 状态 (Status) | 优先级 (Priority) | 说明 (Note) |
| :--- | :--- | :--- | :--- |
| **Match `else` -> `_`** | ✅ Done | High | 已统一为 `_` 通配符默认分支 |
| **Inner 变量可见性** | ✅ Fixed | High | 已改为默认私有，使用 `pub` 标记公开；移除 `inner` 关键字 |
| **枚举方法扩展** | ✅ Fixed | High | `Enum does { ... }` 扩展方法可被运行时查找调用 |
| **预留关键字** | ✅ Fixed | Medium | `async`/`await`/`can` 已预留为关键字，禁止作标识符 |
| **单语句块简写 `:`** | ✅ Done | Low | `:` 后解析单条语句；多语句块仍用 `{}` |
| **Import 命名空间** | ✅ Fixed | Medium | `use` 不再隐式合并导出到当前作用域 |

---

## 1. Match 语法重构 (Match Syntax Refactoring)

### 现状 (Current State)
`match` 的默认分支已使用通配符 `_` 模式（Default Branch）。

```xu
match status {
    Status#pending { ... }
    else {
        // Handle all other cases
    }
}
```

### 目标 (Target State)
为了统一语法并增强模式匹配的一致性，我们将**移除 `match` 中的 `else` 分支支持**，转而强制使用标准的通配符 `_` 模式来处理默认情况。

```xu
match status {
    Status#pending { ... }
    _ {
        // Handle all other cases
    }
}
```

### 行动计划 (Action Items)
- [x] **Parser 已完成**: `match` 默认分支强制为 `_` 通配符模式。
- [ ] **文档/示例对齐**: 若仍有文档片段使用 `else`，统一改为 `_`。

---

## 2. 单语句块简写：冒号语法 (Single-Statement Block Shorthand)

### 2.1 问题描述
为 `if/while/for/match arm` 提供一种“单语句块”的简写方式，同时保持整体风格：**回车断句，`{}` 确定块**。

### 2.2 评估结论
✅ 已实现：**使用 `:` 引导单条语句作为 body**；多语句块仍然必须使用 `{ ... }`。

### 2.3 详细分析
*   **一致性**: `:` 明确标识“后面是一条 body 语句”，避免无分隔符带来的歧义与糟糕错误信息。
*   **风格契合**: 回车依旧是语句终止符；`{}` 依旧是多语句块的唯一形式。
*   **缩进说明**: 行首空格仅用于排版，不再作为语义缩进块。

### 2.4 改进建议
规则如下：
1.  **单语句使用 `:`**: `:` 后只解析一条语句（可以同一行，也可以换行后写一条语句）。
2.  **多语句使用 `{}`**: 需要多条语句时必须写 `{ ... }`。

```xu
if x > 0: return true

match x {
    0: return "zero",
    _ : return "other"
}
```

---

## 3. 可见性实现 (Visibility Implementation)

### 3.1 核心机制
*   **默认私有**: 所有顶层定义默认为私有（仅本模块可见）。
*   **pub 关键字**: 使用 `pub` 标记公开成员。
*   **运行时强制执行**: 在模块加载时，运行时只导出标记为 `pub` 的成员。

### 3.2 历史变更
*   **已移除**: `inner` 关键字已被移除。
*   **已修复**: `AssignStmt` 已包含 `vis` 字段，Parser 会把可见性写入 AST。
*   **已移除 Hack**: 运行时不再通过源码扫描识别可见性，而是在导出阶段从 AST 收集并过滤。

### 3.3 当前状态
1.  ✅ **AST 重构已完成**: `AssignStmt.vis: Visibility`。
2.  ✅ **Parser 修复已完成**: 默认 `Visibility::Inner`，`pub` 标记为 `Visibility::Public`。
3.  ✅ **移除 Hack 已完成**: 不再依赖 `scan_inner_names_from_source`。

---

## 4. Static 关键字实现分析 (Analysis of Static Keyword Implementation)

### 4.1 核心机制
代码库通过 **编译时名称重整 (Name Mangling)** 来实现静态方法。
*   **Parser 处理**: `static func foo` -> 全局函数 `__static__{Struct}__{Method}`。
*   **AST 提升**: 重整后的函数被提升为模块级全局函数。

### 4.2 限制 (Limitations)
*   **不支持静态字段**: 目前 Parser 仅支持 `static func`，**不支持 `static var`**。
*   **反射受限**: 运行时难以获取静态方法列表。

### 4.3 状态总结
*   **静态方法**: ✅ 已实现。
*   **静态字段**: ❌ 未实现。

---

## 5. Self 关键字实现分析 (Analysis of Self Keyword Implementation)

### 5.1 核心机制
`self` 关键字通过 **语法糖 (Syntactic Desugaring)** 实现。
*   **Parser 注入**: 方法定义若无 `self` 参数，Parser 会自动注入第一个参数 `self`。
*   **调用转换**: `obj.method()` 在运行时被转换为 `func(obj)` 调用。

### 5.2 状态总结
*   **Self 注入**: ✅ 已实现。
*   **方法调用绑定**: ✅ 已实现。

---

## 6. 类型定义与扩展分析 (Has/With/Does Implementation)

### 6.1 核心机制
*   **Has (Struct)**: 定义结构体，方法重整为 `__method__{Type}__{Name}`。
*   **Does (Extension)**: 扩展类型，方法同样重整为 `__method__{Type}__{Name}`。
*   **运行时分发**: 通过拼接 `Type` + `Method` 查找全局函数，实现统一分发。

### 6.2 缺陷与限制
*   **枚举方法扩展**: ✅ **已实现**。运行时会优先查找 `__method__{Enum}__{Method}` 的用户扩展方法，找不到再回退内置方法（Option/Result 等）。
*   **内置类型扩展**: ❌ **受限**。Parser 硬编码禁止扩展 `int` 等内置类型。

---

## 7. Let 与 Var 关键字分析 (Analysis of Let/Var Implementation)

### 7.1 核心机制
不可变性通过 **运行时检查 (Runtime Enforcement)** 实现。
*   **Environment**: 维护 `mut_flags` 表记录变量可变性。
*   **运行时检查**: 每次赋值前检查 `is_immutable` 标志。

### 7.2 缺点
*   **检查滞后**: 错误只能在运行时发现。
*   **性能开销**: 赋值操作有额外开销。
*   **建议**: 引入语义分析阶段进行静态检查。

---

## 8. When 关键字分析 (Analysis of When Keyword Implementation)

### 8.1 核心机制
`when` 是 **Parser 级语法糖**，直接脱糖为嵌套的 `match` 语句。
*   **功能**: 专门用于 `Option` 和 `Result` 的解包。
*   **多绑定**: 支持 `when x=a, y=b`，转换为嵌套 match。

### 8.2 关键风险
*   **依赖 Match 的默认分支**: `when` 脱糖生成的代码依赖 `match` 的 `_` 默认分支。
*   **一致性要求**: 调整 `match` 默认分支语法/语义时，必须同步更新 `when` 的脱糖逻辑。

---

## 9. Use 与 As 关键字分析 (Analysis of Use/As Implementation)

### 9.1 核心机制
*   **加载**: `import_path` 负责加载和编译模块。
*   **绑定**: 将模块对象绑定到别名变量。

### 9.2 潜在问题
*   **命名空间污染**: ✅ 已避免。`use` 不再隐式把模块导出合并进当前作用域；需要通过 `alias.member` 访问导出成员。

---

## 10. 其他关键字实现分析 (Analysis of Remaining Keywords)

### 10.1 已实现关键字
*   **控制流**: `if`, `else`, `while`, `for`, `in`, `break`, `continue`, `return`。
*   **逻辑/字面量**: `true`, `false`, `not`, `and`, `or`, `is`, `isnt`。
*   **函数**: `func`。

### 10.2 预留关键字 (Reserved Keywords)
规范中声明的 `can`, `async`, `await` 在代码中 **已预留**。
*   **现状**: Lexer 识别为关键字 Token，Parser 在需要标识符处会给出明确错误。
*   **影响**: 这些词不再允许作为变量/函数/字段名，避免未来引入异步/能力系统时产生更大的破坏性变更。

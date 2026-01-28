# 序语言 (XuLang) v0.1

序语言是一门中文结构化编程语言，目标是“结构化、无歧义、可执行”，适合 AI 生成与人类审核协作场景。当前版本提供完整的 Rust 实现与命令行工具 `xu`。

## 项目总览

- 架构分层：
  - `xu_syntax`：Span/Source/Token/Diagnostic 等基础类型
  - `xu_lexer`：源码归一化与词法分析（含缩进块 INDENT/DEDENT、标点符号规则）
  - `xu_parser`：表达式与语句解析、错误恢复、AST 生成
  - `xu_driver`：前端编排与静态分析、字节码编译
  - `xu_ir`：AST/Bytecode/Executable 等共享 IR
  - `xu_runtime`：解释执行、作用域与闭包、异常、最小标准库
  - `xu_cli`：命令行入口 `tokens`/`check`/`ast`/`run`
  - `xu_codegen`：将 AST 生成 Python/JS

- 规范与文档：
  - 文档索引（推荐阅读顺序）：[docs/README.md](docs/README.md)
  - 语言规范 v0.1：[docs/序语言规范v0.1.md](docs/序语言规范v0.1.md)
  - 标准库参考 v0.1：[docs/序语言标准库参考v0.1.md](docs/序语言标准库参考v0.1.md)
  - 标准库速查（Cheatsheet）：[docs/序语言标准库速查.md](docs/序语言标准库速查.md)
  - 语言特性提案（规范扩展）：枚举类型（一期）[docs/proposals/枚举类型提案.md](docs/proposals/枚举类型提案.md)
  - CLI 用户指南：[docs/CLI用户指南.md](docs/CLI用户指南.md)
  - 架构文档：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
  - 开发计划：[docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md)
  - 任务清单：[docs/开发任务管理清单.md](docs/开发任务管理清单.md)
  - 测试计划与门禁：
    - [详细测试计划.md](docs/详细测试计划.md)
    - [回归与CI门禁.md](docs/回归与CI门禁.md)
  - Examples 索引：[EXAMPLES索引.md](EXAMPLES索引.md)

## 快速开始

- 依赖：安装 Rust/Cargo（稳定版）
- 本地一键门禁：`cargo run -p xtask -- verify`
- 运行示例：
  - `xu run examples/01_basics.xu`
  - `xu check examples/02_control_flow.xu`
  - 更多命令用法参见 CLI 用户指南

## 命令行（摘要）

- `xu tokens <file>`：打印 Token 流
- `xu check <file>`：词法 + 语法检查（出现 Error 时退出码非 0）
- `xu ast <file>`：输出 AST（用于人工验收）
- `xu run <file>`：解释执行（运行时 v1）
- 退出码：无参/缺参为 2；成功为 0；错误为非 0
- 诊断格式：`Severity:line:col: message`，并在可定位时附带源码行与 `^` 指示

## 示例与回归

- 示例目录：[examples](examples)
- Examples 回归：`cargo run -p xtask -- examples`（或 `bash scripts/verify_examples.sh`）
- Golden 更新：见 [GOLDEN维护指南.md](GOLDEN维护指南.md)

## 开发者指南（简要）

- 质量门禁：提交前需通过 `fmt/clippy/test` 与 examples 回归脚本
- 诊断与定位：列号按“字符列”计算（中文友好），尽量携带 `Span`
- 作用域策略：缩进块不引入新作用域；函数调用引入新作用域并支持环境捕获
- 插值：`"...{expr}..."` 支持任意表达式，按当前环境求值并拼接
- 模块：
  - 语句形式：`引入 "path"。`（用于副作用加载）
  - 表达式形式：`模块 为 引入("path")。`，返回模块对象（模块类型）；同时将导出合并进当前环境；可通过 `模块 的 名称` 访问导出（导出为函数时调用可写作 `模块 函数名(...)` 或 `(模块 的 函数名)(...)`）
  - 导出集合：模块顶层符号（不含内置函数；以下划线 `_` 开头的符号不导出）
  - 路径解析：相对路径优先以“导入方文件所在目录”解析，找不到时回退到“当前工作目录”；内部会进行绝对路径规范化并按规范化路径缓存
  - 循环引入：运行时抛错并输出路径链条（便于定位）


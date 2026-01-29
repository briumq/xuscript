# 序语言 (XuLang) v1.1

序语言 (Xu) 是一门强类型、结构化的脚本语言，设计目标是“结构化、无歧义、可执行”。v1.1 版本采用英文关键字，提供完整的 Rust 实现与命令行工具。

## 项目总览

### 架构分层
- `xu_syntax`：Span/Source/Token/Diagnostic 等基础类型
- `xu_lexer`：源码归一化与词法分析
- `xu_parser`：表达式与语句解析、错误恢复、AST 生成
- `xu_driver`：前端编排与静态分析、字节码编译
- `xu_ir`：AST/Bytecode/Executable 等共享 IR
- `xu_runtime`：解释执行、作用域与闭包、异常、最小标准库
- `xu_cli`：命令行入口 `tokens`/`check`/`ast`/`run`

### 规范与文档
- **语言规范**：[docs/Xu 语言规范 v1.1.md](docs/Xu%20语言规范%20v1.1.md)
- **标准库参考**：[docs/标准库参考 v1.1.md](docs/标准库参考%20v1.1.md)
- **语法定义**：[docs/序语言语法定义v1.1.md](docs/序语言语法定义v1.1.md)
- **测试指南**：[docs/Xu 语言测试用例 v1.1.md](docs/Xu%20语言测试用例%20v1.1.md)
- **未完成功能跟踪**：[docs/未完成功能跟踪列表.md](docs/未完成功能跟踪列表.md)

## 快速开始

### 依赖
- 安装 Rust/Cargo（稳定版）

### 运行
1. **本地一键门禁**：
   ```bash
   cargo run -p xtask -- verify
   ```

2. **运行示例**：
   ```bash
   # 运行脚本
   cargo run -- run examples/01_basics.xu
   
   # 语法检查
   cargo run -- check examples/02_control_flow.xu
   ```

## 命令行工具

- `xu tokens <file>`：打印 Token 流
- `xu check <file>`：词法 + 语法检查
- `xu ast <file>`：输出 AST
- `xu run <file>`：解释执行

## 开发者指南

### 语法概览
```xu
use "math"

func main() {
    let list = [1, 2, 3]
    if list.len() > 0 {
        println("List is not empty")
    }
    
    for i in list {
        println("Item: {i}")
    }
}
```

### 模块系统
- **导入**：`use "path"` 或 `use "path" as alias`。
- **导出**：默认导出所有顶层符号，使用 `inner` 修饰符隐藏。
- **路径解析**：优先相对于当前文件解析。

### 质量门禁
提交前请确保通过所有测试：
```bash
cargo run -p xtask -- verify
```

更多详细信息请参阅 [docs/](docs/) 目录下的文档。

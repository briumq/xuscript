# xtask

工程任务入口：用 Rust 代码封装日常开发任务（fmt/clippy/test/examples/perf 等），避免依赖复杂脚本与平台差异。

## 在整体架构中的位置

- 对用户不可见（开发者工具）
- 通过 `cargo run -p xtask -- <cmd>` 调用
- 常用于 CI 本地复现与一键回归

## 命令概览

- `verify`：组合执行 fmt/clippy/test/examples/可选项目回归
- `fmt`：格式检查
- `clippy` / `lint`：lint（严格模式会开启更多 clippy 组）
- `test`：workspace 全量测试
- `examples`：对 `examples/` 做 check/ast/run 回归
- `codegen-examples`：对示例做 JS/Python codegen，并可选运行生成产物
- `perf`：性能基准相关（可选更新 baseline）
- `bench-report`：生成基准对比报告

入口实现：`src/main.rs`。

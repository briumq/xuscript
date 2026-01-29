# Golden 测试维护指南（序语言 v0.1）

生成日期：2026-01-21

## 位置与类型

- Parser 快照：`crates/xu_parser/tests/golden_snapshots.rs`
  - `tokens` 快照：`crates/xu_parser/tests/golden/tokens/*.txt`
  - `ast` 快照：`crates/xu_parser/tests/golden/ast/*.txt`
  - `diagnostics` 快照：`crates/xu_parser/tests/golden/diagnostics/*.txt`
- Runtime/集成快照：`crates/xu_runtime/tests/runner.rs`（统一 runner）
  - `integration`：`crates/xu_runtime/tests/golden/integration/*.txt`
  - `integration_en`：`crates/xu_runtime/tests/golden/integration_en/*.txt`
  - `examples`：`crates/xu_runtime/tests/golden/examples/*.txt`

## 更新流程

- 开启写回：设置环境变量 `XU_UPDATE_GOLDEN=1`（兼容 `HAOSCRIPT_UPDATE_GOLDEN=1`）
- 运行测试：`cargo test -p xu_parser --test golden_snapshots` 或 `cargo test -p xu_runtime --test runner`
- 只更新单个用例：在命令末尾追加测试名（例如 `... golden_ast_01_basics`）
- 验证一致性：关闭写回变量后再次 `cargo test` 进行严格比对（`trim_end` 已对齐尾部换行差异）

## 收敛策略

- 尽量避免对易变输出做快照（随机顺序、非关键空间差异）
- 诊断输出保持稳定：格式、排序按 `span.start`/严重级别等固定规则
- 变更时提交说明：明确语义变化 vs 格式调整，附带对应快照差异摘要

# Xu 语言测试指南

本目录包含 Xu 语言的测试用例集合；统一的测试入口在 `crates/xu_runtime/tests/runner.rs`。

## 目录结构

- specs/: 语言规范测试（主要依赖语言内断言，自证正确性）
- edge/: 编译器与 VM 边界用例（AST 与 VM 输出一致性）
- integration/: 集成用例（对比 golden 输出，基线的一部分）
- integration_en/: 可选的英文版集成用例（对比 golden 输出，若目录不存在会自动跳过）
- benchmarks/: 性能基准相关套件
- ../examples/: 示例（同样纳入 golden 基线，对比输出）

## 运行测试

统一 runner：`crates/xu_runtime/tests/runner.rs`

运行所有正确性测试：

```bash
cargo test -p xu_runtime --test runner
```

更新 golden 文件（新增用例或输出变化时使用）：

```bash
XU_UPDATE_GOLDEN=1 cargo test -p xu_runtime --test runner
```

## Suite 开关（环境变量）

- XU_TEST_EXAMPLES=0|false：跳过整个 examples 套件（首次跑不起来时可临时关闭）
- XU_TEST_EXAMPLES_INCLUDE_EXPECT_FAIL=1|true：额外运行 examples/manifest.json 里标记为 run_expect_fail 的示例
- XU_TEST_EDGE=0|false：跳过 edge 套件
- XU_TEST_DRAFTS=1：启用 specs/v1_1_drafts（若目录存在）
- XU_TEST_ONLY=<substr>：只跑文件名（stem）或 suite 名包含该子串的用例
- XU_TEST_SKIP=<csv>：跳过路径包含任意子串的用例（例如 csv_importer,large/）

## Benchmarks

冒烟测试（小规模）：

```bash
cargo test -p xu_runtime --test run_benchmarks
```

完整基准（通常配合 --ignored）：

```bash
cargo test -p xu_runtime --test perf_benchmarks -- --ignored
```

## Scripts

- scripts/run_cross_lang_bench.sh [SCALE]
- scripts/bench_report.py

## 添加新用例

1. specs/: 添加带断言的 .xu
2. edge/: 添加 AST/VM 一致性用例
3. integration/ 或 examples/: 添加 .xu，并用 XU_UPDATE_GOLDEN=1 生成/更新基线输出

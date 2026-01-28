本目录为 `xu_runtime` 的集成测试集合，按用途大致分为：

- `run_*`：运行期行为与输出（含 golden）
- `perf_*`：性能门禁与基准（通常通过 `--ignored` + `xtask perf` 跑）
- `import_*`：模块/引入/缓存相关行为
- `*_check`：内置函数/能力表等一致性检查
- `language_spec_*`：面向语言规范的行为覆盖测试


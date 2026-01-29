# Examples（v1.1 对齐）

- 结构与命名
  - 中等体量：name.xu（英文语义名，无数字前缀）
  - 超大体量：large/project_name/，唯一入口脚本命名为语义化文件名（避免 main 冲突）
- 运行方式
  - 使用 CLI：`xu run examples/<name>.xu`
  - 大体量：`xu run examples/large/<project>/<entry>.xu`
- 门禁与黄金文件
  - 测试 runner 已纳入 examples 套件，输出与黄金文件对比（tests/golden/examples）
- 列表（新增）
  - 中等体量：log_pipeline_enhanced、config_layered_merge、ecommerce_checkout_pricing、user_signup_validation_flow、order_state_machine_full、router_controller_di、time_series_rolling_stats、path_glob_and_batchops、rule_engine_dsl、job_scheduler_basic、auth_token_verifier、json_transformer、csv_importer、cache_lru_impl、graph_topo_sort、string_template_engine
  - 超大体量：large/site_gen、large/mini_sql、large/orchestrator、large/data_pipeline、large/event_sourcing、large/search_indexer

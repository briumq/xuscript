# Benchmarks Report (Python vs Node.js vs Xu)

- Generated: 1769800095
- Runs per scale: 1
- Scales: 5000
- OS: darwin / arm64
- Rust: rustc 1.85.0 (4d91de4e4 2025-02-17)
- Python: Python 3.9.6
- Node: v22.14.0

## Scale 5000

| case | Python median (ms) | Node.js median (ms) | Xu median (ms) | Python p95 | Node p95 | Xu p95 | Python op/s | Node op/s | Xu op/s | jitter | Py mem (MB) | Node mem (MB) | Xu mem (MB) | winner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| loop | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| dict | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| dict-intkey | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| dict-hot | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| string | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| string-builder | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| struct-method | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| func-call | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| branch-heavy | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| try-catch | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| list-push-pop | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| dict-miss | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| dict-update-hot | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| string-unicode | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| string-scan | - | - | - | - | - | - | - | - | - | - | - | - | - | - |

### Notes
- Numbers are medians over repeated runs of the same per-case microbenchmark.

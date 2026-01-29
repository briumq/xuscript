# Benchmarks Report (Python vs Node.js vs Xu)

- Generated: 1769702431
- Runs per scale: 3
- Scales: 50000
- OS: darwin / arm64
- Rust: rustc 1.85.0 (4d91de4e4 2025-02-17)
- Python: Python 3.9.6
- Node: v22.14.0

## Scale 50000

| case | Python median (ms) | Node.js median (ms) | Xu median (ms) | Python p95 | Node p95 | Xu p95 | Python op/s | Node op/s | Xu op/s | jitter | Py mem (MB) | Node mem (MB) | Xu mem (MB) | winner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| loop | 1.05 | 0.31 | - | 1.05 | 0.31 | - | 47600144 | 158877436 | - | - | 11.47 | 38.27 | - | Node.js |
| dict | 10.44 | 12.62 | - | 10.44 | 12.62 | - | 4788966 | 3961468 | - | - | 20.92 | 54.62 | - | Python |
| dict-intkey | 2.68 | 1.00 | - | 2.68 | 1.00 | - | 18671814 | 50085546 | - | - | 20.92 | 55.83 | - | Node.js |
| dict-hot | 1.28 | 0.41 | - | 1.28 | 0.41 | - | 39059967 | 121310346 | - | - | 22.50 | 67.45 | - | Node.js |
| string | 4.29 | 3.45 | - | 4.29 | 3.45 | - | 11664300 | 14497481 | - | - | 22.50 | 58.72 | - | Node.js |
| string-builder | 4.12 | 1.40 | - | 4.12 | 1.40 | - | 12124151 | 35801642 | - | - | 22.50 | 60.88 | - | Node.js |
| struct-method | 3.16 | 0.52 | - | 3.16 | 0.52 | - | 15844511 | 95306171 | - | - | 22.50 | 67.50 | - | Node.js |
| func-call | 2.55 | 0.45 | - | 2.55 | 0.45 | - | 19637685 | 111607143 | - | - | 22.50 | 67.53 | - | Node.js |
| branch-heavy | 1.61 | 0.35 | - | 1.61 | 0.35 | - | 31112251 | 141894289 | - | - | 22.50 | 67.55 | - | Node.js |
| try-catch | - | - | - | - | - | - | - | - | - | - | - | - | - | - |
| list-push-pop | 2.07 | 0.90 | - | 2.07 | 0.90 | - | 24096873 | 55845144 | - | - | 22.50 | 68.62 | - | Node.js |
| dict-miss | 4.70 | 11.58 | - | 4.70 | 11.58 | - | 10643581 | 4316966 | - | - | 22.50 | 61.84 | - | Python |
| dict-update-hot | 0.80 | 0.28 | - | 0.80 | 0.28 | - | 62827251 | 177410027 | - | - | 22.50 | 61.86 | - | Node.js |
| string-unicode | 1.21 | 2.09 | - | 1.21 | 2.09 | - | 41227185 | 23868730 | - | - | 22.50 | 64.05 | - | Python |
| string-scan | 0.00 | 0.01 | - | 0.00 | 0.01 | - | 13950892857 | 9023641942 | - | - | 22.50 | 66.05 | - | Python |

### Notes
- Numbers are medians over repeated runs of the same per-case microbenchmark.

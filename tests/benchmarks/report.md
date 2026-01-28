# Benchmarks Report

- Generated: 1769095672
- Scales: 5000, 10000
- OS: macos / aarch64
- Rust: rustc 1.91.0 (f8297e351 2025-10-28)
- Python: Python 3.9.6
- Node: v22.14.0

## Scale 5000

| case | Python (ms) | Node.js (ms) | Xu (ms) | winner |
|---|---:|---:|---:|---|
| loop | 0.19 | 0.07 | <1 | Xu |
| dict | 1.98 | 2.75 | 5 | Python |
| dict-intkey | 0.56 | 0.27 | 2 | Node.js |
| dict-hot | 0.31 | 0.15 | 4 | Node.js |
| string | 0.95 | 1.15 | 31 | Python |
| string-builder | 0.93 | 0.53 | 3 | Node.js |
| struct-method | 0.72 | 0.42 | 12 | Node.js |

## Scale 10000

| case | Python (ms) | Node.js (ms) | Xu (ms) | winner |
|---|---:|---:|---:|---|
| loop | 0.45 | 0.16 | 1 | Node.js |
| dict | 5.39 | 9.02 | 11 | Python |
| dict-intkey | 1.32 | 0.71 | 5 | Node.js |
| dict-hot | 0.68 | 0.25 | 5 | Node.js |
| string | 2.39 | 2.28 | 101 | Node.js |
| string-builder | 2.34 | 0.98 | 4 | Node.js |
| struct-method | 1.71 | 0.75 | 8 | Node.js |


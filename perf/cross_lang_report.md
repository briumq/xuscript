# Cross-language Benchmarks Report

- Generated: 1769568543
- Scales: 10000, 50000, 100000
- OS: macos / aarch64
- Rust: rustc 1.85.0 (4d91de4e4 2025-02-17)
- Python: Python 3.9.6
- Node: v22.14.0

## Summary

| scale | worst case | Xu vs best | Xu (ms) | best (ms) |
|---:|---|---:|---:|---:|
| 10000 | struct-method | 5.38 | 1 | 0.19 |
| 50000 | struct-method | 7.94 | 6 | 0.76 |
| 100000 | struct-method | 12.29 | 12 | 0.98 |

## Scale 10000

| case | Python (ms) | Node.js (ms) | Xu (ms) | Xu/Py | Xu/Node | winner |
|---|---:|---:|---:|---:|---:|---|
| loop | 0.17 | 0.06 | <1 | <0.01 | <0.01 | Xu |
| dict-build | 0.86 | 0.84 | 2 | 2.33 | 2.39 | Node.js |
| dict | 1.83 | 2.79 | 5 | 2.74 | 1.79 | Python |
| dict-intkey | 0.49 | 0.43 | 1 | 2.05 | 2.34 | Node.js |
| dict-hot | 0.29 | 0.15 | <1 | <0.01 | <0.01 | Xu |
| string | 0.84 | 0.48 | 1 | 1.20 | 2.08 | Node.js |
| string-builder | 0.80 | 0.37 | 1 | 1.25 | 2.71 | Node.js |
| struct-method | 0.63 | 0.19 | 1 | 1.59 | 5.38 | Node.js |

## Scale 50000

| case | Python (ms) | Node.js (ms) | Xu (ms) | Xu/Py | Xu/Node | winner |
|---|---:|---:|---:|---:|---:|---|
| loop | 0.82 | 0.47 | 1 | 1.21 | 2.15 | Node.js |
| dict-build | 5.01 | 4.82 | 11 | 2.19 | 2.28 | Node.js |
| dict | 9.44 | 16.97 | 25 | 2.65 | 1.47 | Python |
| dict-intkey | 2.63 | 0.90 | 5 | 1.90 | 5.57 | Node.js |
| dict-hot | 1.35 | 0.44 | 1 | 0.74 | 2.27 | Node.js |
| string | 4.23 | 2.69 | 5 | 1.18 | 1.86 | Node.js |
| string-builder | 4.15 | 1.53 | 4 | 0.96 | 2.62 | Node.js |
| struct-method | 3.17 | 0.76 | 6 | 1.89 | 7.94 | Node.js |

## Scale 100000

| case | Python (ms) | Node.js (ms) | Xu (ms) | Xu/Py | Xu/Node | winner |
|---|---:|---:|---:|---:|---:|---|
| loop | 1.73 | 0.53 | 2 | 1.16 | 3.79 | Node.js |
| dict-build | 9.97 | 10.75 | 23 | 2.31 | 2.14 | Python |
| dict | 20.65 | 29.36 | 51 | 2.47 | 1.74 | Python |
| dict-intkey | 5.37 | 4.89 | 10 | 1.86 | 2.05 | Node.js |
| dict-hot | 2.84 | 1.07 | 2 | 0.70 | 1.87 | Node.js |
| string | 8.39 | 4.45 | 10 | 1.19 | 2.25 | Node.js |
| string-builder | 8.36 | 4.70 | 8 | 0.96 | 1.70 | Node.js |
| struct-method | 6.43 | 0.98 | 12 | 1.87 | 12.29 | Node.js |


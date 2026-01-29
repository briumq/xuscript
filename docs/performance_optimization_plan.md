# 性能优化方案 (Performance Optimization Plan)

基于 `tests/benchmarks/report.md` 的基准测试结果，我们识别出 Xu 语言在字典操作和字符串处理方面存在显著的性能瓶颈。以下是针对性的优化方案。

## 1. 瓶颈分析 (Analysis)

### 1.1 字符串扫描极慢 (`string-scan`)
*   **现象**: `string-scan` 场景比 Python 慢 144 倍 (Xu 0.33ms vs Py 0.00ms)。
*   **原因**: `str.rs` 中的 `contains`, `starts_with` 等方法在每次调用时都会**克隆**接收者 (`self`) 和参数字符串。
    ```rust
    // 伪代码
    let s = self.clone(); // ❌ 昂贵的堆内存分配与拷贝
    let sub = arg.clone();
    s.contains(&sub)
    ```
    在紧密循环中，这种无意义的内存分配导致了极高的开销。

### 1.2 字典热点访问慢 (`dict-hot`)
*   **现象**: `dict-hot` 场景比 Node.js 慢 10 倍以上。
*   **原因**:
    1.  **String Cloning**: `dict.rs` 在 `get`/`insert` 时，如果 Key 是字符串，会克隆该字符串用于查找或插入。
    2.  **Hashing Overhead**: 每次访问都需要重新计算字符串 Hash。
    3.  **Lack of Interning**: 相同的字符串字面量在内存中存在多份副本，无法进行指针比较。

## 2. 优化策略 (Optimization Strategies)

### 2.1 消除字符串克隆 (Zero-Copy String Access)
**目标**: 在 Runtime 方法分发中，避免克隆字符串数据。

*   **方案**: 重构 `Method` 签名或 `Runtime` 的借用规则，使得内置方法能够以 `&str` 借用的方式访问 Heap 中的字符串，而不是获取所有权 (`String`)。
*   **预期收益**: `string-scan`, `string-methods` 性能提升 10x-100x。

### 2.2 字符串驻留 (String Interning)
**目标**: 优化字典 Key 的存储与比较。

*   **方案**:
    1.  引入全局或线程局部的 `Interner`。
    2.  将所有作为 Dict Key 的字符串转换为唯一的 `Symbol` (u64 ID)。
    3.  Dict 底层存储改为 `HashMap<Symbol, Value>`。
*   **预期收益**:
    *   字符串比较变为整数比较 (O(1))。
    *   Hash 计算变为整数 Hash (极快)。
    *   `dict-hot` 性能大幅提升。

### 2.3 引入 Inline Cache (IC)
**目标**: 加速属性访问和方法调用。

*   **方案**:
    *   在 Bytecode 中为 `GetItem`, `SetItem`, `CallMethod` 预留 IC 槽位。
    *   **Dict IC**: 缓存 `(DictID, Version, LastIndex)`。如果下次访问时 Dict ID 和 Version 未变，直接通过 Index 访问底层数组（如果 Dict 实现改为 Open Addressing 或类似结构）。
    *   **Method IC**: 缓存 `(TypeID, MethodPtr)`，避免重复的哈希查找。

### 2.4 优化对象布局 (Object Layout)
*   **方案**: 考虑将 `Dict` 的存储从 `HashMap` (Std/HashBrown) 迁移到更紧凑的自定义布局，或者优化 `HashBrown` 的使用方式（如使用 `raw_entry` API 避免重复 Key 克隆，目前代码中已部分使用，但仍有优化空间）。

## 3. 实施计划 (Roadmap)

1.  **Phase 1: Zero-Copy (P0)**
    *   修改 `str.rs` 和 `dict.rs`，移除不必要的 `clone()`。
    *   这是投入产出比最高的优化。

2.  **Phase 2: String Interning (P1)**
    *   实现 `Interner`。
    *   改造 `Dict` 使用 Symbol 作为 Key。

3.  **Phase 3: Inline Caching (P2)**
    *   为解释器循环引入 IC 机制。

## 4. 验证标准
*   `string-scan` 耗时降低至 0.01ms 级。
*   `dict-hot` 耗时降低至 0.1ms 级。

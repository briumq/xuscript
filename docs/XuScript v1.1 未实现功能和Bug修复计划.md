# XuScript v1.1 Bug修复和功能实现计划

## 优先修复的运行时Bug

1. **内联match表达式赋值失败**
   - 问题：`let val = match expr { ... }` 报错 `RuntimeError: Format requires Dict argument`
   - 解决方案：修复match表达式的内联赋值逻辑，确保正确处理表达式结果
   - 验证：创建测试用例验证内联match表达式赋值

2. **Result.map返回结构体失败**
   - 问题：`result.map(|x| -> Struct { Struct{...} })` 报错 `RuntimeError: Unsupported member access: field on type unit`
   - 解决方案：修复Result.map方法处理结构体返回值的逻辑，确保正确传递结构体
   - 验证：创建测试用例验证Result.map返回结构体

3. **to_float()无效输入panic**
   - 问题：`"invalid".to_float()` 直接panic
   - 解决方案：修改为返回Result或添加try_to_float()方法，提供错误处理机制
   - 验证：创建测试用例验证无效输入的处理

## 功能实现

1. **添加push作为add的别名**
   - 问题：当前只支持add方法，不符合通用编程语言惯例
   - 解决方案：在列表实现中添加push方法作为add的别名
   - 验证：创建测试用例验证push方法功能

2. **元组解构和多返回值解构**
   - 问题：仅顶层可用，函数内有bug
   - 解决方案：修复函数内的元组解构逻辑，确保在任何作用域都能使用
   - 验证：创建测试用例验证函数内的元组解构

3. **字典键值对循环**
   - 问题：`for (key, value) in map` 未实现
   - 解决方案：实现字典的键值对迭代功能
   - 验证：创建测试用例验证字典键值对循环

4. **整数.to_string()方法**
   - 问题：未实现，只能使用字符串插值
   - 解决方案：实现整数到字符串的转换方法
   - 验证：创建测试用例验证整数.to_string()方法

## 示例代码修复

1. **修复列表操作**：将所有`list.push(x)`改为`list.add(x)`，同时保持push作为别名
2. **修复变量声明**：将所有`let mut x = ...`改为`var x = ...`
3. **移除类型转换**：移除所有`expr as Type`语法
4. **修复数学函数**：将`Math.random()`改为`use "math"`后使用`math.random()`
5. **修复字符串空检查**：将`str.none`改为`str.length == 0`或`str == ""`
6. **修复字典大小访问**：将`dict.size`改为`dict.length`
7. **修复when语句**：为所有`when ... { }`添加else分支
8. **移除结构体继承**：使用组合替代继承

## 实施步骤

1. **Bug修复阶段**
   - 修复内联match表达式赋值失败问题
   - 修复Result.map返回结构体失败问题
   - 修复to_float()无效输入panic问题

2. **功能实现阶段**
   - 添加push作为add的别名
   - 实现完整的元组解构和多返回值解构
   - 实现字典键值对循环
   - 实现整数.to_string()方法

3. **示例代码修复阶段**
   - 批量修复所有示例文件中的不符合规范的代码
   - 验证修复后的示例代码能够正常运行

4. **验证阶段**
   - 运行完整的测试套件，确保所有问题都已修复
   - 验证示例代码能够正常运行
   - 更新规范对齐检查清单，标记已完成的项目
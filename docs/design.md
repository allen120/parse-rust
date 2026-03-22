# parse-rust 设计文档

## 1. 项目背景

### 1.1 原始项目

[parse](https://github.com/r1chardj0n3s/parse) 是一个 Python 字符串解析库，提供与 `str.format()` 相反的操作。给定一个格式模板和实际字符串，自动提取其中的变量值。该库约 1.8k Stars，广泛用于日志分析、数据提取、ETL 管道等场景。

### 1.2 重写动机

| 动机 | 说明 |
|------|------|
| **性能** | 原版纯 Python 实现，字符串处理和正则匹配密集，是 Rust 擅长的领域 |
| **安全性** | 原版处理的输入往往来自外部不可信数据，Rust 的内存安全可消除缓冲区溢出风险 |
| **无现有替代** | Rust 生态中不存在功能等价的 Python format-spec 解析库 |

### 1.3 技术选型

- **语言**: Rust（系统级性能 + 内存安全保证）
- **Python 绑定**: PyO3 + Maturin（成熟的 Rust-Python 互操作方案）
- **正则引擎**: Rust `regex` crate（比 Python `re` 快 10-50x）

## 2. 架构设计

### 2.1 总体架构

```
┌─────────────────────────────────────────────┐
│              Python Layer (PyO3)             │
│  parse() / search() / findall() / compile() │
└────────────────────┬────────────────────────┘
                     │
┌────────────────────▼────────────────────────┐
│              Rust Core Engine                │
│                                              │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  │
│  │ Compiler │→│  Parser  │→│  Result   │  │
│  │          │  │          │  │           │  │
│  │ Format → │  │ Regex    │  │ Fixed +   │  │
│  │ Regex    │  │ Match +  │  │ Named     │  │
│  │ Pattern  │  │ Extract  │  │ Values    │  │
│  └────┬─────┘  └──────────┘  └───────────┘  │
│       │                                      │
│  ┌────▼─────┐                                │
│  │  Types   │                                │
│  │          │                                │
│  │ :d → Int │                                │
│  │ :f → F64 │                                │
│  │ :w → Str │                                │
│  │ ...      │                                │
│  └──────────┘                                │
└──────────────────────────────────────────────┘
```

### 2.2 模块职责

| 模块 | 文件 | 职责 |
|------|------|------|
| **Compiler** | `compiler.rs` | 将 format 字符串编译为正则表达式模式 |
| **Types** | `types.rs` | 定义所有格式说明符、正则模式和类型转换逻辑 |
| **Parser** | `parser.rs` | 编排编译和匹配流程，提供 parse/search/findall API |
| **Result** | `result.rs` | 封装解析结果，支持按索引和按名称访问 |
| **PyO3 Bindings** | `lib.rs` | 通过 PyO3 将 Rust 函数暴露为 Python 可调用接口 |

### 2.3 数据流

```
Format String: "User {name:w} is {:d} years old"
                          │
                    ┌─────▼─────┐
                    │  split()  │  分割为文本和字段
                    └─────┬─────┘
                          │
              ┌───────────▼───────────┐
              │  handle_field()       │  每个字段 → 正则捕获组
              │  "name:w" → (?P<name>\w+)
              │  ":d"     → ([-+ ]?\d+)
              └───────────┬───────────┘
                          │
                  ┌───────▼───────┐
                  │ Regex Pattern │
                  │ "(?i)User (?P<name>\w+) is ([-+ ]?\d+) years old"
                  └───────┬───────┘
                          │
Input String: "User Alice is 30 years old"
                          │
                  ┌───────▼───────┐
                  │ regex.match() │
                  └───────┬───────┘
                          │
              ┌───────────▼───────────┐
              │ evaluate_captures()   │
              │ name="Alice" → Str    │
              │ group1="30"  → Int(30)│
              └───────────┬───────────┘
                          │
                  ┌───────▼───────┐
                  │  ParseResult  │
                  │ fixed: [30]   │
                  │ named: {name: "Alice"} │
                  └───────────────┘
```

## 3. 核心实现细节

### 3.1 Format 字符串编译器

编译器将 Python format mini-language 转换为正则表达式：

```
[[fill]align][sign][0][width][grouping][.precision][type]
```

支持的对齐方式：`<`（左对齐）、`>`（右对齐）、`^`（居中）、`=`（填充在符号后）

### 3.2 类型系统

实现了全部 25 种格式说明符：

- **文本类型** (7种): default, s, w, W, l, S, D
- **整数类型** (5种): d, b, o, x, n
- **浮点类型** (5种): f, F, e, g, %
- **日期时间类型** (8种): ti, te, ta, tg, th, tc, tt, ts

每种类型定义了：
1. 匹配用正则模式
2. 额外捕获组数量
3. 字符串到目标类型的转换函数

### 3.3 ParseValue 枚举

```rust
enum ParseValue {
    Str(String),      // 文本类型
    Int(i64),         // 整数类型
    Float(f64),       // 浮点类型
    DateTime(String), // 日期时间（保留原始字符串）
    Percent(f64),     // 百分比（已除以100）
}
```

### 3.4 PyO3 绑定

通过 PyO3 实现完全的 API 兼容：

| Python API | Rust 实现 |
|-----------|----------|
| `parse(fmt, str)` | `parser::parse()` |
| `search(fmt, str)` | `parser::search()` |
| `findall(fmt, str)` | `parser::findall()` |
| `compile(fmt)` | `Parser::new()` |
| `result[0]` | `PyParseResult.__getitem__()` |
| `result["name"]` | `PyParseResult.__getitem__()` |
| `result.fixed` | `PyParseResult.fixed` (getter) |
| `result.named` | `PyParseResult.named` (getter) |

## 4. 性能优化策略

### 4.1 已实现的优化

1. **Rust regex 引擎**: 使用 `regex` crate 替代 Python `re`，基于 NFA/DFA 混合引擎
2. **零拷贝字符串处理**: 尽可能使用 `&str` 避免不必要的内存分配
3. **编译缓存**: `Parser` 对象编译一次，匹配多次

### 4.2 计划中的优化

1. **LRU 缓存**: 对 `parse()` 便捷函数添加格式字符串编译缓存
2. **SIMD 加速**: 利用 Rust regex crate 的 SIMD 优化
3. **并行解析**: 支持批量字符串的并行解析

## 5. 安全性提升

### 5.1 内存安全

Rust 的所有权系统在编译期保证：
- 无缓冲区溢出
- 无悬垂指针
- 无数据竞争

### 5.2 输入验证

- 所有外部输入经过 Rust 类型系统验证
- 正则表达式编译失败返回明确错误
- 整数溢出通过 Rust 的检查算术处理

## 6. 测试策略

### 6.1 单元测试

每个模块都有独立的单元测试：
- `types::tests` — 类型转换正确性
- `compiler::tests` — format 字符串编译正确性
- `parser::tests` — 解析功能正确性

### 6.2 集成测试

Python 集成测试确保与原版 API 兼容性。

### 6.3 基准测试

对比 Python 原版和 Rust 重写版在以下场景的性能：
- 单次解析延迟
- 批量解析吞吐量
- 内存占用

## 7. 开发路线图

| 阶段 | 内容 | 状态 |
|------|------|------|
| Phase 1 | 核心解析引擎（compiler + types + parser） | 已完成 |
| Phase 2 | PyO3 Python 绑定 | 已完成 |
| Phase 3 | 完整单元测试 | 已完成 |
| Phase 4 | 性能基准测试 | 进行中 |
| Phase 5 | 高级功能（自定义类型、嵌套字段） | 计划中 |
| Phase 6 | PyPI 发布 | 计划中 |

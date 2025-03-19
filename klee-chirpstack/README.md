# ChirpStack模块化KLEE分析

本项目提供了一个框架，用于对ChirpStack的各个模块进行KLEE符号执行分析，帮助检测内存错误、逻辑漏洞和协议合规性问题。

## 目录结构

```
klee-chirpstack/
├── aes128_driver.c          # AES128模块的C驱动
├── build_and_run.sh         # C驱动构建和执行脚本
├── klee_entry.rs            # 纯Rust的KLEE分析入口点
├── build_and_run_rust.sh    # Rust驱动构建和执行脚本
├── module_template.c        # 新C模块分析的模板
└── README.md                # 本说明文件
```

## 前提条件

- LLVM/Clang (推荐11.0+)
- KLEE 符号执行引擎
- Rust (nightly版本推荐)
- cargo-klee (可选)

## 使用方法

### 1. 分析AES128模块 (C驱动)

这种方式使用C语言驱动，通过FFI调用Rust函数：

```bash
chmod +x build_and_run.sh
./build_and_run.sh
```

运行后，分析结果将保存在`aes128/reports/`目录中。

### 2. 分析AES128模块 (纯Rust)

这种方式直接在Rust中进行符号执行，无需通过C FFI：

```bash
chmod +x build_and_run_rust.sh
./build_and_run_rust.sh
```

运行后，分析结果将保存在`aes128_rust/reports/`目录中。

### 3. 分析其他模块

#### 使用C驱动

1. 复制并修改C模板文件：

```bash
cp module_template.c new_module_driver.c
```

2. 修改Rust源文件，添加必要的KLEE支持代码和FFI导出函数
3. 复制并修改构建脚本：

```bash
cp build_and_run.sh build_new_module.sh
```

4. 调整新脚本中的模块路径和分析参数
5. 运行新脚本

#### 使用纯Rust

1. 创建新的Rust入口文件，参考`klee_entry.rs`的结构
2. 修改入口文件中的测试案例
3. 复制并修改Rust构建脚本：

```bash
cp build_and_run_rust.sh build_new_module_rust.sh
```

4. 调整新脚本中的模块路径和分析参数
5. 运行新脚本

## C驱动 vs 纯Rust分析

两种方法的对比：

| 特性              | C驱动           | 纯Rust          |
|-------------------|----------------|-----------------|
| 实现复杂度        | 较高            | 较低            |
| 分析准确性        | 可能有FFI问题    | 更准确          |
| 性能              | 有FFI开销       | 无FFI开销       |
| 支持范围          | 所有公开函数    | 所有功能        |
| 调试难度          | 较难            | 较容易          |

一般建议：
- 对于简单模块，使用纯Rust分析
- 对于涉及C互操作的模块，使用C驱动更接近真实场景

## 解释KLEE结果

KLEE分析会生成以下结果：

- **测试用例 (.ktest)**: 可以使用`ktest-tool`查看
- **错误报告 (.err)**: 包含检测到的错误详情
- **统计信息**: 包含代码覆盖率、分析时间等
- **错误摘要**: 汇总所有错误，按严重程度分类

## 常见问题解决

### 路径爆炸问题

如果遇到路径爆炸，可以尝试：

1. 减小符号输入的大小
2. 增加约束条件
3. 使用`--max-forks=N`限制路径分叉数量
4. 尝试不同的搜索策略 (DFS, BFS, Random-Path等)

### 编译错误

确保：

1. Rust模块中的`klee_analysis`特性已正确配置
2. FFI导出函数正确声明(#[no_mangle])
3. KLEE包含路径正确设置

### Rust链接问题

如遇到Rust链接问题：

1. 检查Cargo.toml中的依赖路径是否正确
2. 确保已编译lrwn库并生成rlib文件
3. 使用`-L`和`--extern`参数显式指定库的路径

## 添加新模块的最佳实践

1. 从小且独立的模块开始
2. 优先分析安全关键模块
3. 先实现简单的属性测试，再逐步添加复杂测试
4. 保持C驱动和Rust代码的同步

## 贡献

欢迎贡献更多模块的分析驱动和改进现有框架。 
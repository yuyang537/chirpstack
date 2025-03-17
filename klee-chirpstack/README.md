# ChirpStack KLEE符号执行分析

本项目使用KLEE符号执行工具对ChirpStack LoRaWAN网络服务器进行安全分析。

## 概述

KLEE是一个符号执行工具，可以自动生成高覆盖率的测试用例，并发现程序中的错误。在本项目中，我们使用KLEE对ChirpStack的关键安全组件进行分析，包括：

- 消息完整性码(MIC)验证
- 加密密钥生成和管理
- 帧计数器(FCnt)验证
- 数据包加密和解密
- 设备认证和授权

## 快速开始

我们提供了自动化脚本来简化KLEE分析的过程。根据您的操作系统，可以使用以下脚本：

### Linux/Docker环境

```bash
# 确保脚本具有执行权限
chmod +x klee-chirpstack/run_klee_analysis.sh

# 运行分析脚本
./klee-chirpstack/run_klee_analysis.sh
```

### Windows环境

```cmd
# 运行分析脚本
klee-chirpstack\run_klee_analysis.bat
```

这些脚本会自动执行以下操作：
1. 检查KLEE是否已安装
2. 安装Rust（如果尚未安装）
3. 安装LLVM工具链
4. 修改必要的配置文件
5. 编译ChirpStack为LLVM位码
6. 运行KLEE符号执行分析
7. 分析并显示结果

## 安装KLEE

要使用KLEE，您需要先安装KLEE及其依赖项。以下是在Ubuntu系统上安装KLEE的步骤：

1. 安装依赖项：

```bash
sudo apt-get update
sudo apt-get install build-essential curl libcap-dev git cmake libncurses5-dev python-minimal python-pip unzip libtcmalloc-minimal4 libgoogle-perftools-dev libsqlite3-dev doxygen
```

2. 安装LLVM和Clang：

```bash
sudo apt-get install llvm-11 llvm-11-dev llvm-11-tools clang-11
```

3. 克隆KLEE仓库：

```bash
git clone https://github.com/klee/klee.git
cd klee
```

4. 构建KLEE：

```bash
mkdir build
cd build
cmake -DENABLE_SOLVER_STP=ON -DENABLE_POSIX_RUNTIME=ON -DENABLE_KLEE_UCLIBC=ON -DKLEE_UCLIBC_PATH=../klee-uclibc -DLLVM_CONFIG_BINARY=/usr/bin/llvm-config-11 -DLLVMCC=/usr/bin/clang-11 -DLLVMCXX=/usr/bin/clang++-11 ..
make -j$(nproc)
sudo make install
```

### 使用Docker

或者，您可以使用KLEE官方Docker镜像：

```bash
docker pull klee/klee:latest
docker run --rm -ti --ulimit='stack=-1:-1' klee/klee
```

## 手动编译ChirpStack以支持KLEE

如果您想手动编译ChirpStack以支持KLEE，可以按照以下步骤操作：

1. 安装Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

2. 安装LLVM工具链：

```bash
rustup component add llvm-tools-preview
```

3. 修改根目录的`Cargo.toml`，确保`klee-chirpstack`在工作区成员中：

```toml
[workspace]
  resolver = "2"
  members = [
    "chirpstack",
    "chirpstack-integration",
    "lrwn",
    "lrwn-filters",
    "backend",
    "api/rust",
    "klee-chirpstack",
  ]
```

4. 修改`chirpstack/Cargo.toml`，添加对本地`klee-sys`包的引用：

```toml
# 在[dependencies]部分添加
klee-sys = { path = "../klee-chirpstack" }
libc = { version = "0.2" }

# 在[features]部分确保klee特性正确配置
klee = ["klee-sys", "libc", "lrwn/crypto"]
```

5. 编译ChirpStack为LLVM位码：

```bash
cd chirpstack
RUSTFLAGS="-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic" cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc
```

这将在`target/release/deps/`目录下生成LLVM位码文件（`.bc`文件）。

## 运行KLEE分析

使用以下命令运行KLEE分析：

```bash
klee --libc=uclibc --posix-runtime --optimize --emit-all-errors --max-memory=4096 --max-time=3600 target/release/deps/chirpstack-*.bc --config ./configuration --command KleeSymbolicExecution
```

KLEE将执行符号分析，并在`klee-out-*`目录中生成测试用例和错误报告。

## 分析KLEE结果

KLEE生成的结果包括：

1. 测试用例（`klee-out-*/test*.ktest`）：这些是KLEE发现的可能执行路径的具体输入值。
2. 错误报告（`klee-out-*/test*.err`）：这些是KLEE发现的潜在错误，如断言失败、内存错误等。

您可以使用KLEE提供的工具分析这些结果：

```bash
# 查看测试用例
ktest-tool klee-out-*/test000001.ktest

# 生成覆盖率报告
klee-stats klee-out-*/
```

## 安全分析重点

在ChirpStack中，我们重点关注以下安全敏感操作：

1. **MIC验证**：确保消息完整性码验证正确实现，防止消息篡改。
2. **加密操作**：验证加密和解密操作的正确性，确保数据机密性。
3. **帧计数器验证**：检查帧计数器验证逻辑，防止重放攻击。
4. **密钥生成**：分析会话密钥生成过程，确保密钥安全性。
5. **设备认证**：验证设备认证过程，防止未授权访问。

## 已添加的符号执行分析

我们已经在ChirpStack代码中添加了以下符号执行分析：

1. `main.rs`中的`analyze_security_sensitive_operations`函数：分析基本的安全敏感操作。
2. `uplink/join.rs`中的`analyze_join_request_with_klee`函数：分析Join Request处理流程。
3. `uplink/data.rs`中的`analyze_uplink_data_with_klee`函数：分析上行数据处理流程。

这些分析函数使用KLEE符号执行来探索不同的执行路径，并验证安全属性。

## 故障排除

如果您在运行分析时遇到问题，请尝试以下解决方案：

1. **找不到`klee-sys`包**：确保`chirpstack/Cargo.toml`文件中正确引用了本地的`klee-sys`包。
2. **虚拟清单错误**：确保在`chirpstack`目录中运行`cargo`命令，而不是在根目录。
3. **KLEE未找到**：确保KLEE已正确安装并添加到PATH中。
4. **编译错误**：确保已安装LLVM工具链（`rustup component add llvm-tools-preview`）。

如果问题仍然存在，请检查日志输出以获取更详细的错误信息。 
#!/bin/bash
# 运行KLEE符号执行分析的脚本（适用于KLEE Docker环境）

set -e

echo "在KLEE Docker环境中运行分析..."

# 安装必要的依赖
echo "安装必要的依赖..."
apt-get update || true
apt-get install -y curl build-essential llvm llvm-dev clang || true

# 安装Rust（如果尚未安装）
if ! command -v cargo &> /dev/null; then
    echo "安装Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    
    # 添加到.bashrc以便下次使用
    echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
fi

# 设置Rust环境变量
export PATH="$HOME/.cargo/bin:$PATH"

# 安装LLVM工具链
echo "安装LLVM工具链..."
$HOME/.cargo/bin/rustup component add llvm-tools-preview

# 进入ChirpStack目录
cd ..

# 编译ChirpStack为LLVM位码
echo "编译ChirpStack为LLVM位码..."
RUSTFLAGS="-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic" $HOME/.cargo/bin/cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc

# 查找生成的位码文件
BITCODE_FILE=$(find target/release/deps -name "chirpstack-*.bc" | head -n 1)
if [ -z "$BITCODE_FILE" ]; then
    echo "错误: 未找到位码文件。编译可能失败。"
    exit 1
fi

echo "找到位码文件: $BITCODE_FILE"

# 创建配置目录
mkdir -p configuration

# 运行KLEE分析
echo "运行KLEE符号执行分析..."
klee --libc=uclibc --posix-runtime --optimize --emit-all-errors --max-memory=4096 --max-time=3600 "$BITCODE_FILE" --config ./configuration --command KleeSymbolicExecution

# 分析结果
echo "分析KLEE结果..."
klee-stats klee-out-*/ --print-all

echo "KLEE分析完成。结果保存在klee-out-*目录中。"
echo "您可以使用ktest-tool查看具体的测试用例，例如:"
echo "ktest-tool klee-out-*/test000001.ktest" 
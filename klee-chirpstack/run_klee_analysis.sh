#!/bin/bash
# 运行KLEE符号执行分析的脚本

set -e

# 检查KLEE是否已安装
if ! command -v klee &> /dev/null; then
    echo "错误: KLEE未安装。请先安装KLEE。"
    exit 1
fi

# 检查LLVM工具链是否已安装
if ! rustup component list --installed | grep -q "llvm-tools"; then
    echo "安装LLVM工具链..."
    rustup component add llvm-tools-preview
fi

# 进入ChirpStack目录
cd ..

# 编译ChirpStack为LLVM位码
echo "编译ChirpStack为LLVM位码..."
RUSTFLAGS="-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic" cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc

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
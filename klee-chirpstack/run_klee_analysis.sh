#!/bin/bash
# 运行KLEE符号执行分析的脚本（适用于KLEE Docker环境）

set -e

echo "在KLEE Docker环境中运行分析..."

# 检查KLEE是否可用
if ! command -v klee &> /dev/null; then
    echo "错误: KLEE未找到。请确保您在klee用户下运行此脚本。"
    exit 1
fi

# 安装Rust（如果尚未安装）
if ! command -v cargo &> /dev/null; then
    echo "安装Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    
    # 加载Rust环境
    . "$HOME/.cargo/env"
    
    # 添加到.bashrc以便下次使用
    echo '. "$HOME/.cargo/env"' >> $HOME/.bashrc
fi

# 确保Rust环境已加载
if ! command -v cargo &> /dev/null; then
    echo "加载Rust环境..."
    . "$HOME/.cargo/env"
fi

# 设置Rust环境变量
export PATH="$HOME/.cargo/bin:$PATH"

# 安装LLVM工具链
echo "安装LLVM工具链..."
rustup component add llvm-tools-preview

# 进入ChirpStack目录
cd ..

# 修改根目录的Cargo.toml，添加klee-chirpstack到工作区
echo "修改根目录的Cargo.toml，添加klee-chirpstack到工作区..."
if ! grep -q "klee-chirpstack" Cargo.toml; then
    # 备份原始文件
    cp Cargo.toml Cargo.toml.bak
    
    # 添加klee-chirpstack到工作区
    sed -i 's/\[workspace\]/\[workspace\]\n  resolver = "2"\n  members = \[\n    "chirpstack",\n    "chirpstack-integration",\n    "lrwn",\n    "lrwn-filters",\n    "backend",\n    "api\/rust",\n    "klee-chirpstack",\n  \]/' Cargo.toml
fi

# 修改chirpstack/Cargo.toml，使其引用本地的klee-sys包
echo "修改chirpstack/Cargo.toml，使其引用本地的klee-sys包..."
if grep -q "klee-sys.*=.*{.*path.*=.*\"..\/klee-chirpstack\"" chirpstack/Cargo.toml; then
    # 已经修改过，不需要再次修改
    echo "chirpstack/Cargo.toml已经引用本地的klee-sys包"
else
    # 备份原始文件
    cp chirpstack/Cargo.toml chirpstack/Cargo.toml.bak
    
    # 修改klee-sys的引用
    sed -i 's/klee-sys.*=.*{.*version.*=.*"0.1.0".*}/klee-sys = { path = "..\/klee-chirpstack" }/' chirpstack/Cargo.toml
    
    # 如果上面的替换失败（可能是因为格式不同），尝试添加新的引用
    if ! grep -q "klee-sys.*=.*{.*path.*=.*\"..\/klee-chirpstack\"" chirpstack/Cargo.toml; then
        sed -i '/\[dependencies\]/a klee-sys = { path = "../klee-chirpstack" }' chirpstack/Cargo.toml
    fi
    
    # 确保klee特性正确配置
    if ! grep -q "klee.*=.*\[\"klee-sys\"" chirpstack/Cargo.toml; then
        sed -i '/\[features\]/,/\[/ s/klee.*=.*\[.*\]/klee = ["klee-sys", "libc", "lrwn\/crypto"]/' chirpstack/Cargo.toml
    fi
fi

# 进入实际的包目录
echo "进入chirpstack包目录..."
cd chirpstack

# 编译ChirpStack为LLVM位码
echo "编译ChirpStack为LLVM位码..."
RUSTFLAGS="-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic" cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc

# 查找生成的位码文件
BITCODE_FILE=$(find ../target/release/deps -name "chirpstack-*.bc" | head -n 1)
if [ -z "$BITCODE_FILE" ]; then
    echo "错误: 未找到位码文件。编译可能失败。"
    exit 1
fi

echo "找到位码文件: $BITCODE_FILE"

# 返回到根目录
cd ..

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
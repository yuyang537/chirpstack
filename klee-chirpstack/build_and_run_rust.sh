#!/bin/bash
# ChirpStack模块化KLEE分析 - 纯Rust版本

# 错误处理
set -e
trap 'echo "错误: 第$LINENO行命令执行失败"; exit 1' ERR

# 颜色输出
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 配置信息
RUST_PROJ_ROOT="../"  # 项目根目录
RUST_ENTRY_FILE="klee_entry.rs"
KLEE_MODULE_DIR="aes128_rust"
KLEE_INCLUDE_PATH="/usr/local/include"  # 修改为您的KLEE include路径
LLVM_PATH="/usr/bin"  # 修改为您的LLVM bin路径

# 创建输出目录
mkdir -p "$KLEE_MODULE_DIR"
cd "$KLEE_MODULE_DIR"

echo -e "${BLUE}=== 开始ChirpStack AES128模块纯Rust KLEE分析 ===${NC}"

# 1. 确保lrwn路径可以被找到
echo -e "${GREEN}[1/4] 设置Rust编译环境...${NC}"

# 创建临时manifest文件，使得我们可以使用lrwn crate
cat > Cargo.toml <<EOF
[package]
name = "klee_rust_analysis"
version = "0.1.0"
edition = "2021"

[dependencies]
lrwn = { path = "$RUST_PROJ_ROOT/lrwn", features = ["klee_analysis"] }

[lib]
name = "klee_rust_analysis"
path = "$RUST_ENTRY_FILE"
crate-type = ["staticlib"]
EOF

# 2. 编译Rust入口文件为LLVM IR
echo -e "${GREEN}[2/4] 编译Rust测试入口到LLVM IR...${NC}"
rustc --crate-type=lib \
      --emit=llvm-bc \
      -L "$RUST_PROJ_ROOT/target/debug/deps" \
      -L "$RUST_PROJ_ROOT/lrwn/target/debug/deps" \
      --extern lrwn="$RUST_PROJ_ROOT/target/debug/deps/liblrwn.rlib" \
      -C panic=abort \
      -C debuginfo=2 \
      -C opt-level=0 \
      --cfg feature=\"klee_analysis\" \
      "$RUST_ENTRY_FILE" -o klee_rust_entry.bc

# 或者使用cargo编译
# RUSTFLAGS="--emit=llvm-ir -C debuginfo=2 -C opt-level=0 -C panic=abort --cfg feature=\"klee_analysis\"" \
# cargo build

# 3. 使用KLEE进行符号执行
echo -e "${GREEN}[3/4] 运行KLEE符号执行...${NC}"

# 根据模块特性设置KLEE参数
KLEE_PARAMS="--libc=uclibc --posix-runtime --emit-all-errors"
KLEE_PARAMS+=" --max-time=3600 --max-memory=2048"
KLEE_PARAMS+=" --output-dir=klee-out"

# 创建KLEE配置文件
cat > klee.config <<EOF
# KLEE配置
max-solver-time=10
max-memory=2048MB
max-time=3600s
search=dfs
EOF

# 运行KLEE
klee $KLEE_PARAMS klee_rust_entry.bc

# 4. 生成分析报告
echo -e "${GREEN}[4/4] 生成分析报告...${NC}"

# 运行klee-stats
mkdir -p reports
klee-stats klee-out/ > reports/klee_stats.txt

# 检索错误报告
if [ -d "klee-out/test-cases" ]; then
  echo -e "${GREEN}发现测试用例，生成测试报告...${NC}"
  find klee-out/test* -name "*.err" -o -name "*.ktest" | sort > reports/test_cases.txt
fi

# 标记错误类型
ERROR_COUNT=$(find klee-out -name "*.err" | wc -l)
if [ "$ERROR_COUNT" -gt 0 ]; then
  echo -e "${RED}发现 $ERROR_COUNT 个错误/警告${NC}"
  
  # 生成错误摘要
  echo "ChirpStack AES128 模块Rust KLEE分析错误摘要" > reports/error_summary.md
  echo "====================================" >> reports/error_summary.md
  echo "" >> reports/error_summary.md
  echo "| 错误类型 | 文件 | 位置 | 严重性 |" >> reports/error_summary.md
  echo "| -------- | ---- | ---- | ------ |" >> reports/error_summary.md
  
  # 解析错误文件
  for ERR_FILE in $(find klee-out -name "*.err"); do
    ERROR_TYPE=$(head -1 "$ERR_FILE")
    ERROR_LOC=$(grep -A 2 "Stack:" "$ERR_FILE" | tail -1)
    SEVERITY="高"
    if [[ "$ERROR_TYPE" == *"assertion"* ]]; then
      SEVERITY="中"
    fi
    echo "| $ERROR_TYPE | $ERR_FILE | $ERROR_LOC | $SEVERITY |" >> reports/error_summary.md
  done
else
  echo -e "${GREEN}未发现错误，AES128模块符号执行通过${NC}"
  echo "ChirpStack AES128 模块Rust KLEE分析 - 无错误" > reports/error_summary.md
fi

echo -e "${BLUE}=== ChirpStack AES128模块Rust KLEE分析完成 ===${NC}"
echo "结果保存在: $(pwd)/reports/" 
#!/bin/bash
# ChirpStack模块化KLEE分析构建和运行脚本

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
MODULE_PATH="lrwn/src/aes128.rs"
KLEE_MODULE_DIR="aes128"
KLEE_INCLUDE_PATH="/usr/local/include"  # 修改为您的KLEE include路径
LLVM_PATH="/usr/lib/llvm-14/bin"  # 修改为您的LLVM bin路径

# 创建输出目录
mkdir -p "$KLEE_MODULE_DIR"
cd "$KLEE_MODULE_DIR"

echo -e "${BLUE}=== 开始ChirpStack AES128模块KLEE分析 ===${NC}"

# 1. 编译Rust文件到LLVM IR
echo -e "${GREEN}[1/5] 编译Rust模块到LLVM IR...${NC}"
cd "$RUST_PROJ_ROOT"

# 确保有klee_analysis特性
if ! grep -q 'klee_analysis' lrwn/Cargo.toml; then
  echo 'klee_analysis = []' >> lrwn/Cargo.toml
fi

# 使用cargo-klee编译（如果安装了）
if command -v cargo-klee &> /dev/null; then
  cargo klee --features klee_analysis --target-dir=target/klee lrwn
  RUST_BC_FILE="target/klee/debug/deps/lrwn-*.bc"
else
  # 手动编译
  RUSTFLAGS="--emit=llvm-ir -C debuginfo=2 -C opt-level=0 -C no-prepopulate-passes -C passes=name-anon-globals --cfg=feature=\"klee_analysis\"" \
  cargo rustc --features klee_analysis --target-dir=target/klee -p lrwn
  
  # 找到生成的IR文件
  RUST_IR_FILE=$(find target/klee/debug/deps -name "lrwn-*.ll" | head -1)
  
  # 转换为bitcode
  "$LLVM_PATH/llvm-as" "$RUST_IR_FILE" -o lrwn.bc
  RUST_BC_FILE="lrwn.bc"
fi

cd -  # 返回到KLEE模块目录

# 复制bitcode文件
cp "$RUST_PROJ_ROOT/$RUST_BC_FILE" ./lrwn_module.bc

# 2. 编译C驱动代码
echo -e "${GREEN}[2/5] 编译KLEE驱动代码...${NC}"
"$LLVM_PATH/clang" -I "$KLEE_INCLUDE_PATH" \
  -emit-llvm -c -g -O0 \
  ../aes128_driver.c -o aes128_driver.bc

# 3. 链接Rust模块和C驱动
echo -e "${GREEN}[3/5] 链接Rust模块和C驱动...${NC}"
"$LLVM_PATH/llvm-link" lrwn_module.bc aes128_driver.bc -o aes128_linked.bc

# 4. 使用KLEE进行符号执行
echo -e "${GREEN}[4/5] 运行KLEE符号执行...${NC}"

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
klee $KLEE_PARAMS --optimize aes128_linked.bc

# 5. 生成分析报告
echo -e "${GREEN}[5/5] 生成分析报告...${NC}"

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
  echo "ChirpStack AES128 模块KLEE分析错误摘要" > reports/error_summary.md
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
  echo "ChirpStack AES128 模块KLEE分析 - 无错误" > reports/error_summary.md
fi

echo -e "${BLUE}=== ChirpStack AES128模块KLEE分析完成 ===${NC}"
echo "结果保存在: $(pwd)/reports/" 
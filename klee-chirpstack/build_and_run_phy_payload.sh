#!/bin/bash
# ChirpStack PhyPayload模块KLEE分析脚本

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
RUST_ENTRY_FILE="phy_payload_klee_entry.rs"
KLEE_MODULE_DIR="phy_payload_klee"
KLEE_INCLUDE_PATH="/usr/local/include"  # 修改为您的KLEE include路径
LLVM_PATH="/usr/bin"  # 修改为您的LLVM bin路径

# 创建输出目录
mkdir -p "$KLEE_MODULE_DIR"
cd "$KLEE_MODULE_DIR"

echo -e "${BLUE}=== 开始ChirpStack PhyPayload模块KLEE分析 ===${NC}"

# 1. 确保lrwn路径可以被找到
echo -e "${GREEN}[1/4] 设置Rust编译环境...${NC}"

# 创建临时manifest文件，使得我们可以使用lrwn crate
cat > Cargo.toml <<EOF
[package]
name = "klee_phy_payload_analysis"
version = "0.1.0"
edition = "2021"

[dependencies]
lrwn = { path = "$RUST_PROJ_ROOT/lrwn", features = ["klee_analysis", "crypto"] }

[lib]
name = "klee_phy_payload_analysis"
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
      --cfg feature=\"crypto\" \
      "$RUST_ENTRY_FILE" -o phy_payload_klee.bc

# 3. 使用KLEE进行符号执行
echo -e "${GREEN}[3/4] 运行KLEE符号执行...${NC}"

# 对于phy_payload这样的复杂模块，我们需要设置更加保守的KLEE参数
KLEE_PARAMS="--libc=uclibc --posix-runtime --emit-all-errors"
KLEE_PARAMS+=" --max-time=7200 --max-memory=4096"  # 更长的时间和更多内存
KLEE_PARAMS+=" --max-solver-time=30"  # 增加求解器时间
KLEE_PARAMS+=" --output-dir=klee-out"
KLEE_PARAMS+=" --search=bfs"  # 使用广度优先搜索
KLEE_PARAMS+=" --max-forks=5000"  # 限制分叉数量以避免路径爆炸

# 创建KLEE配置文件
cat > klee.config <<EOF
# KLEE配置
max-solver-time=30
max-memory=4096MB
max-time=7200s
search=bfs
max-forks=5000
EOF

# 运行KLEE - 确保添加--optimize选项进行优化
echo -e "${GREEN}运行KLEE符号执行 (这可能需要很长时间)...${NC}"
klee $KLEE_PARAMS --optimize phy_payload_klee.bc

# 4. 生成分析报告
echo -e "${GREEN}[4/4] 生成分析报告...${NC}"

# 运行klee-stats
mkdir -p reports
klee-stats klee-out/ > reports/klee_stats.txt

# 计算代码覆盖率
if command -v kcov &> /dev/null; then
  echo -e "${GREEN}计算代码覆盖率...${NC}"
  kcov --include-pattern=lrwn/src/phy_payload.rs coverage/ klee-out/
  echo "覆盖率报告生成在: $(pwd)/coverage/"
fi

# 检索错误报告
if [ -d "klee-out/test-cases" ]; then
  echo -e "${GREEN}发现测试用例，生成测试报告...${NC}"
  find klee-out/test* -name "*.err" -o -name "*.ktest" | sort > reports/test_cases.txt
fi

# 分析错误报告
# 分析错误报告
ERROR_COUNT=$(find klee-out -name "*.err" | wc -l)
if [ "$ERROR_COUNT" -gt 0 ]; then
  echo -e "${RED}发现 $ERROR_COUNT 个错误/警告${NC}"
  
  # 创建更详细的错误报告
  echo "# ChirpStack PhyPayload 模块KLEE分析错误报告" > reports/error_report.md
  echo "" >> reports/error_report.md
  echo "## 摘要" >> reports/error_report.md
  echo "" >> reports/error_report.md
  echo "- 总共检测到 **$ERROR_COUNT** 个错误或警告" >> reports/error_report.md
  echo "- 分析时间: $(date)" >> reports/error_report.md
  echo "" >> reports/error_report.md
  
  # 按错误类型分类
  echo "## 按类型分类的错误" >> reports/error_report.md
  echo "" >> reports/error_report.md
  echo "| 错误类型 | 数量 | 严重程度 |" >> reports/error_report.md
  echo "| -------- | ---- | ------- |" >> reports/error_report.md
  
  # 提取唯一错误类型并统计
  ERROR_TYPES=$(grep -h "^[A-Z]" klee-out/*.err | sort | uniq -c | sort -nr)
  echo "$ERROR_TYPES" | while read -r count type; do
    SEVERITY="高"
    if [[ "$type" == *"assert"* ]]; then
      SEVERITY="中"
    elif [[ "$type" == *"overflow"* || "$type" == *"memory"* ]]; then
      SEVERITY="高"
    else
      SEVERITY="低"
    fi
    echo "| $type | $count | $SEVERITY |" >> reports/error_report.md
  done
  
  echo "" >> reports/error_report.md
  echo "## 详细错误列表" >> reports/error_report.md
  echo "" >> reports/error_report.md
  
  # 列出所有错误
  find klee-out -name "*.err" | sort | while read -r ERR_FILE; do
    ERROR_TYPE=$(head -1 "$ERR_FILE")
    ERROR_LOC=$(grep -A 2 "Stack:" "$ERR_FILE" | tail -1)
    TEST_CASE=$(basename "$ERR_FILE" .err)
    
    echo "### 错误: $ERROR_TYPE" >> reports/error_report.md
    echo "" >> reports/error_report.md
    echo "- **位置**: \`$ERROR_LOC\`" >> reports/error_report.md
    echo "- **测试用例**: $TEST_CASE" >> reports/error_report.md
    echo "- **复现命令**: \`ktest-tool klee-out/$TEST_CASE.ktest\`" >> reports/error_report.md
    echo "" >> reports/error_report.md
    echo "\`\`\`" >> reports/error_report.md
    cat "$ERR_FILE" | head -20 >> reports/error_report.md
    echo "...(省略部分内容)" >> reports/error_report.md
    echo "\`\`\`" >> reports/error_report.md
    echo "" >> reports/error_report.md
  done
else
  echo -e "${GREEN}未发现错误，PhyPayload模块符号执行通过${NC}"
  echo "# ChirpStack PhyPayload 模块KLEE分析 - 无错误" > reports/error_report.md
  echo "" >> reports/error_report.md
  echo "符号执行完成，未发现任何错误。" >> reports/error_report.md
  echo "" >> reports/error_report.md
  echo "统计信息:" >> reports/error_report.md
  echo "\`\`\`" >> reports/error_report.md
  cat reports/klee_stats.txt >> reports/error_report.md
  echo "\`\`\`" >> reports/error_report.md
fi

# 添加一些额外信息到报告中
echo "" >> reports/error_report.md
echo "## KLEE执行统计" >> reports/error_report.md
echo "" >> reports/error_report.md
echo "\`\`\`" >> reports/error_report.md
cat reports/klee_stats.txt >> reports/error_report.md
echo "\`\`\`" >> reports/error_report.md

# 添加安全建议
echo "" >> reports/error_report.md
echo "## 安全建议" >> reports/error_report.md
echo "" >> reports/error_report.md
echo "根据分析结果，以下是提高PhyPayload模块安全性的建议：" >> reports/error_report.md
echo "" >> reports/error_report.md
echo "1. 确保所有MIC计算使用安全的密钥派生方法" >> reports/error_report.md
echo "2. 实现额外的输入验证，防止恶意输入" >> reports/error_report.md
echo "3. 增加针对重放攻击的额外保护措施" >> reports/error_report.md
echo "4. 考虑添加时间固定的实现，防止侧信道攻击" >> reports/error_report.md
echo "" >> reports/error_report.md

echo -e "${BLUE}=== ChirpStack PhyPayload模块KLEE分析完成 ===${NC}"
echo "结果保存在: $(pwd)/reports/" 
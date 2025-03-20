#!/bin/bash
# 安装KLEE运行时库

set -e

echo "=== 安装KLEE运行时库 ==="

# 确定KLEE安装位置
KLEE_BIN=$(which klee)
if [ -z "$KLEE_BIN" ]; then
  echo "错误: 找不到KLEE。请确保KLEE已正确安装。"
  exit 1
fi

KLEE_DIR=$(dirname $(dirname $KLEE_BIN))
echo "KLEE安装目录: $KLEE_DIR"

# 检查运行时库是否存在
LIB_DIR="$KLEE_DIR/lib/klee/runtime"
if [ ! -d "$LIB_DIR" ]; then
  echo "警告: 找不到KLEE运行时库目录。尝试创建..."
  sudo mkdir -p "$LIB_DIR"
fi

# 如果找不到KLEE源码，尝试获取
if [ ! -d "klee-src" ]; then
  echo "下载KLEE源码..."
  git clone https://github.com/klee/klee.git klee-src
  cd klee-src
  
  # 配置KLEE
  echo "配置KLEE..."
  mkdir -p build
  cd build
  cmake -DENABLE_POSIX_RUNTIME=ON ..
  
  # 编译运行时库
  echo "编译KLEE运行时库..."
  make kleeRuntest
  
  # 安装运行时库
  echo "安装KLEE运行时库..."
  sudo cp runtime/lib/* "$LIB_DIR/"
  
  cd ../..
  echo "KLEE运行时库安装完成。"
else
  echo "KLEE源码目录已存在。跳过下载步骤。"
  cd klee-src/build
  
  # 重新编译和安装
  echo "编译和安装KLEE运行时库..."
  make kleeRuntest
  sudo cp runtime/lib/* "$LIB_DIR/"
  
  cd ../..
  echo "KLEE运行时库安装完成。"
fi

# 创建符号链接
echo "创建符号链接..."
sudo ln -sf "$LIB_DIR/libkleeRuntimePOSIX.bca" "/usr/local/lib/klee/runtime/"
sudo ln -sf "$LIB_DIR/libkleeRuntimePOSIX_Debug+Asserts.bca" "/usr/local/lib/klee/runtime/"
sudo ln -sf "$LIB_DIR/libkleeRuntimePOSIX.bc" "/usr/local/lib/klee/runtime/"
sudo ln -sf "$LIB_DIR/libkleeRuntimePOSIX_Debug+Asserts.bc" "/usr/local/lib/klee/runtime/"

echo "=== KLEE运行时库安装完成 ===" 
#!/bin/bash
# 简化的AES128 KLEE分析脚本

set -e
LLVM_BIN="/usr/lib/llvm-14/bin"
KLEE_INCLUDE="/usr/local/include"

echo "=== 开始编译AES128 C驱动 ==="
$LLVM_BIN/clang -I $KLEE_INCLUDE -emit-llvm -c -g -O0 aes128_driver.c -o aes128_driver.bc

echo "=== 构建lrwn库 ==="
cd ..
cargo build -p lrwn --features="klee_analysis"

echo "=== 将aes128.rs编译为LLVM位码 ==="
rustc --crate-type=lib \
      --emit=llvm-bc \
      -C debuginfo=2 \
      -C opt-level=0 \
      -C panic=abort \
      --cfg feature=\"klee_analysis\" \
      lrwn/src/aes128.rs -o aes128.bc

echo "=== 链接Rust和C驱动 ==="
cd klee-chirpstack
$LLVM_BIN/llvm-link ../aes128.bc aes128_driver.bc -o aes128_linked.bc

echo "=== 运行KLEE符号执行 ==="
klee --libc=uclibc --posix-runtime --emit-all-errors aes128_linked.bc

echo "=== 分析完成 ==="
echo "结果保存在: klee-last/"
klee-stats klee-last/ 
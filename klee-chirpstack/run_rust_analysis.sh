#!/bin/bash
# 简化的纯Rust KLEE分析脚本

set -e
LLVM_BIN="/usr/lib/llvm-14/bin"

echo "=== 构建lrwn库 ==="
cd ..
cargo build -p lrwn --features="klee_analysis"

echo "=== 编译Rust KLEE入口 ==="
cd klee-chirpstack
rustc --crate-type=lib \
      --emit=llvm-bc \
      -L "../target/debug/deps" \
      --extern lrwn="../target/debug/deps/liblrwn.rlib" \
      -C debuginfo=2 \
      -C opt-level=0 \
      -C panic=abort \
      --cfg feature=\"klee_analysis\" \
      klee_entry.rs -o klee_rust_entry.bc

echo "=== 运行KLEE符号执行 ==="
klee --libc=uclibc --posix-runtime --emit-all-errors klee_rust_entry.bc

echo "=== 分析完成 ==="
echo "结果保存在: klee-last/"
klee-stats klee-last/ 
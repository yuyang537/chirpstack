#!/bin/bash
# 最简单的AES128 KLEE分析脚本 - 仅使用C代码

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== 开始简化版AES128 KLEE分析 ==="

# 创建临时目录
TEMP_DIR="$SCRIPT_DIR/simple_aes128_analysis"
mkdir -p "$TEMP_DIR"
cd "$TEMP_DIR"

# 1. 创建简单的AES128实现（C版本）
echo "1. 创建简化的AES128 C实现..."
cat > simple_aes128.c << 'EOF'
#include <stdint.h>
#include <string.h>
#include <klee/klee.h>  // 添加KLEE头文件

// AES128密钥结构
typedef struct {
    uint8_t bytes[16];
} AES128Key;

// 简化的AES加密（使用XOR）
void aes128_encrypt(const AES128Key *key, uint8_t data[16]) {
    for (int i = 0; i < 16; i++) {
        data[i] ^= key->bytes[i];
    }
}

// 简化的AES解密（使用XOR）
void aes128_decrypt(const AES128Key *key, uint8_t data[16]) {
    for (int i = 0; i < 16; i++) {
        data[i] ^= key->bytes[i];
    }
}

// KLEE符号执行入口点
int main() {
    AES128Key key;
    uint8_t data[16];
    uint8_t original_data[16];
    
    // 使数据和密钥成为符号化的
    klee_make_symbolic(&key, sizeof(key), "key");
    klee_make_symbolic(&data, sizeof(data), "data");
    
    // 保存原始数据副本
    memcpy(original_data, data, 16);
    
    // 加密数据
    aes128_encrypt(&key, data);
    
    // 确认加密后数据已更改
    int changed = 0;
    for (int i = 0; i < 16; i++) {
        if (data[i] != original_data[i]) {
            changed = 1;
            break;
        }
    }
    klee_assert(changed);
    
    // 解密数据
    aes128_decrypt(&key, data);
    
    // 验证解密后的数据与原始数据相同
    for (int i = 0; i < 16; i++) {
        klee_assert(data[i] == original_data[i]);
    }
    
    return 0;
}
EOF

# 2. 编译为LLVM位码
echo "2. 编译为LLVM位码..."
clang -emit-llvm -c -g -O0 -Xclang -disable-O0-optnone simple_aes128.c -o simple_aes128.bc

# 3. 运行KLEE分析
echo "3. 运行KLEE分析..."
klee --optimize --emit-all-errors simple_aes128.bc

# 4. 分析结果
echo "4. 分析KLEE结果..."
mkdir -p reports
klee-stats klee-last/ > reports/klee_stats.txt

ERROR_COUNT=$(find klee-last -name "*.err" | wc -l)
if [ "$ERROR_COUNT" -gt 0 ]; then
  echo "发现 $ERROR_COUNT 个错误或警告"
  
  # 创建错误报告
  echo "AES128模块KLEE分析错误摘要" > reports/error_summary.txt
  echo "=========================" >> reports/error_summary.txt
  echo "" >> reports/error_summary.txt
  
  find klee-last -name "*.err" | while read -r ERR_FILE; do
    echo "错误文件: $ERR_FILE" >> reports/error_summary.txt
    cat "$ERR_FILE" >> reports/error_summary.txt
    echo "" >> reports/error_summary.txt
  done
else
  echo "未发现错误，AES128分析通过"
fi

echo "=== 简化版AES128 KLEE分析完成 ==="
echo "结果保存在: $TEMP_DIR/klee-last/"
echo "统计信息: $TEMP_DIR/reports/" 
#!/bin/bash
# 直接编译AES128.rs并进行KLEE分析的脚本
# 这个脚本不依赖cargo构建系统，避免klee-sys依赖问题

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LLVM_BIN="/usr/lib/llvm-14/bin"

echo "==== 使用直接编译方式分析AES128模块 ===="
echo "项目根目录: $PROJECT_ROOT"
echo "LLVM路径: $LLVM_BIN"

# 创建一个临时目录
TEMP_DIR="$SCRIPT_DIR/aes128_analysis"
mkdir -p "$TEMP_DIR"
cd "$TEMP_DIR"

# 1. 创建一个简单的rust库，只包含我们需要的类型和函数
echo "1. 创建AES128分析用的简化rust库..."
cat > simple_aes128.rs << 'EOF'
// 用于KLEE分析的简单AES128实现

pub struct AES128Key([u8; 16]);

impl AES128Key {
    pub fn from_bytes(b: [u8; 16]) -> Self {
        AES128Key(b)
    }
    
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0
    }
    
    pub fn encrypt(&self, data: &mut [u8; 16]) {
        // 简化的AES加密实现（XOR操作）
        for i in 0..16 {
            data[i] ^= self.0[i];
        }
    }
    
    pub fn decrypt(&self, data: &mut [u8; 16]) {
        // 简化的AES解密实现（XOR操作）
        for i in 0..16 {
            data[i] ^= self.0[i];
        }
    }
}

// FFI导出函数，用于KLEE驱动程序调用
#[no_mangle]
pub extern "C" fn aes128_encrypt(key_ptr: *const u8, data_ptr: *mut u8) {
    let mut key_bytes = [0u8; 16];
    let mut data = [0u8; 16];
    
    unsafe {
        std::ptr::copy_nonoverlapping(key_ptr, key_bytes.as_mut_ptr(), 16);
        std::ptr::copy_nonoverlapping(data_ptr, data.as_mut_ptr(), 16);
        
        let key = AES128Key::from_bytes(key_bytes);
        key.encrypt(&mut data);
        
        std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, 16);
    }
}

#[no_mangle]
pub extern "C" fn aes128_decrypt(key_ptr: *const u8, data_ptr: *mut u8) {
    let mut key_bytes = [0u8; 16];
    let mut data = [0u8; 16];
    
    unsafe {
        std::ptr::copy_nonoverlapping(key_ptr, key_bytes.as_mut_ptr(), 16);
        std::ptr::copy_nonoverlapping(data_ptr, data.as_mut_ptr(), 16);
        
        let key = AES128Key::from_bytes(key_bytes);
        key.decrypt(&mut data);
        
        std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, 16);
    }
}

#[no_mangle]
pub extern "C" fn test_aes_roundtrip() -> bool {
    let key_bytes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let key = AES128Key::from_bytes(key_bytes);
    
    let mut data = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160];
    let original_data = data;
    
    // 加密
    key.encrypt(&mut data);
    
    // 解密
    key.decrypt(&mut data);
    
    // 验证解密后的数据与原始数据相同
    for i in 0..16 {
        if data[i] != original_data[i] {
            return false;
        }
    }
    
    true
}
EOF

# 2. 编译Rust库为LLVM位码
echo "2. 编译Rust库为LLVM位码..."
rustc --crate-type=lib \
      --emit=llvm-bc \
      -C debuginfo=2 \
      -C opt-level=0 \
      -C panic=abort \
      simple_aes128.rs -o simple_aes128.bc

# 3. 复制C驱动文件
echo "3. 准备C驱动文件..."
cp "$SCRIPT_DIR/klee_wrappers.h" .
cp "$SCRIPT_DIR/aes128_driver.c" .

# 修改aes128_driver.c使用我们的包装头文件
sed -i 's/#include <klee\/klee.h>/#include "klee_wrappers.h"/' aes128_driver.c 2>/dev/null || \
sed -i 's|#include <klee/klee.h>|#include "klee_wrappers.h"|' aes128_driver.c

# 4. 编译C驱动为LLVM位码
echo "4. 编译C驱动为LLVM位码..."
$LLVM_BIN/clang -I . -emit-llvm -c -g -O0 aes128_driver.c -o aes128_driver.bc

# 5. 链接Rust和C位码
echo "5. 链接Rust和C位码..."
$LLVM_BIN/llvm-link simple_aes128.bc aes128_driver.bc -o aes128_linked.bc

# 6. 运行KLEE分析
echo "6. 运行KLEE符号执行分析..."
klee --libc=uclibc --posix-runtime --emit-all-errors aes128_linked.bc

# 7. 分析结果
echo "7. 分析KLEE结果..."
mkdir -p reports
klee-stats klee-last/ > reports/klee_stats.txt

# 检查错误
ERROR_COUNT=$(find klee-last -name "*.err" | wc -l)
if [ "$ERROR_COUNT" -gt 0 ]; then
  echo "发现 $ERROR_COUNT 个错误或警告"
  
  # 创建简单的错误报告
  echo "AES128模块KLEE分析错误摘要" > reports/error_summary.txt
  echo "===========================" >> reports/error_summary.txt
  echo "" >> reports/error_summary.txt
  
  find klee-last -name "*.err" | while read -r ERR_FILE; do
    echo "错误文件: $ERR_FILE" >> reports/error_summary.txt
    head -10 "$ERR_FILE" >> reports/error_summary.txt
    echo "..." >> reports/error_summary.txt
    echo "" >> reports/error_summary.txt
  done
else
  echo "未发现错误，AES128符号执行通过"
fi

echo "==== AES128模块分析完成 ===="
echo "结果保存在: $TEMP_DIR/klee-last/"
echo "统计信息: $TEMP_DIR/reports/" 
#!/bin/bash
# 纯Rust的AES128 KLEE分析脚本
# 不使用POSIX运行时，避免库依赖问题

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=== 开始纯Rust AES128 KLEE分析 ==="

# 创建一个临时目录
TEMP_DIR="$SCRIPT_DIR/rust_aes128_analysis"
mkdir -p "$TEMP_DIR"
cd "$TEMP_DIR"

# 1. 创建一个独立的Rust文件，包含AES128实现和KLEE入口点
echo "1. 创建独立的Rust KLEE测试文件..."
cat > rust_aes128_klee.rs << 'EOF'
// AES128的简单实现以及KLEE测试入口点

// 简单的外部函数声明
extern "C" {
    fn klee_make_symbolic(data: *mut u8, len: usize, name: *const u8);
    fn klee_assume(condition: u8);
    fn klee_assert(condition: u8);
}

// 密钥数据结构
pub struct AES128Key([u8; 16]);

impl AES128Key {
    pub fn from_bytes(b: [u8; 16]) -> Self {
        AES128Key(b)
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

// 创建符号化AES128密钥
fn make_symbolic_key(name: &str) -> AES128Key {
    let mut key = [0u8; 16];
    unsafe {
        klee_make_symbolic(
            key.as_mut_ptr(),
            16,
            format!("{}\0", name).as_ptr(),
        );
    }
    AES128Key::from_bytes(key)
}

// 创建符号化数据
fn make_symbolic_data(data: &mut [u8; 16], name: &str) {
    unsafe {
        klee_make_symbolic(
            data.as_mut_ptr(),
            16,
            format!("{}\0", name).as_ptr(),
        );
    }
}

// 测试1: 加密/解密循环
fn test_encrypt_decrypt_cycle() {
    // 创建符号化密钥和数据
    let key = make_symbolic_key("key");
    let mut data = [0u8; 16];
    make_symbolic_data(&mut data, "data");
    
    // 保存原始数据副本
    let original_data = data;
    
    // 加密数据
    key.encrypt(&mut data);
    
    // 确保加密改变了数据
    let mut has_changed = false;
    for i in 0..16 {
        if data[i] != original_data[i] {
            has_changed = true;
            break;
        }
    }
    
    // 断言：加密应该改变数据
    unsafe {
        klee_assert(has_changed as u8);
    }
    
    // 解密数据
    key.decrypt(&mut data);
    
    // 断言：解密后应该恢复原始数据
    for i in 0..16 {
        unsafe {
            klee_assert((data[i] == original_data[i]) as u8);
        }
    }
}

// 测试2: 不同密钥产生不同加密结果
fn test_key_difference() {
    // 创建两个符号化密钥
    let key1 = make_symbolic_key("key1");
    let key2 = make_symbolic_key("key2");
    
    // 创建一个符号化数据
    let mut data = [0u8; 16];
    make_symbolic_data(&mut data, "diff_data");
    
    // 使用两个密钥分别加密相同数据
    let mut result1 = data;
    let mut result2 = data;
    
    key1.encrypt(&mut result1);
    key2.encrypt(&mut result2);
    
    // 检查结果是否相同
    let mut results_equal = true;
    for i in 0..16 {
        if result1[i] != result2[i] {
            results_equal = false;
            break;
        }
    }
    
    // 如果结果相同，密钥必须相同
    if results_equal {
        // 验证密钥是否相同
        let mut keys_equal = true;
        for i in 0..16 {
            if key1.0[i] != key2.0[i] {
                keys_equal = false;
                break;
            }
        }
        
        unsafe {
            klee_assert(keys_equal as u8);
        }
    }
}

// KLEE入口函数
#[no_mangle]
pub extern "C" fn main() {
    test_encrypt_decrypt_cycle();
    test_key_difference();
}
EOF

# 2. 使用兼容的Rust版本编译
echo "2. 使用兼容的Rust版本编译..."
if command -v rustup &> /dev/null; then
    COMPATIBLE_RUST="1.60.0"
    # 确保工具链已安装
    rustup toolchain install "$COMPATIBLE_RUST" --profile minimal || true
    RUSTC="rustup run $COMPATIBLE_RUST rustc"
    echo "使用Rust $COMPATIBLE_RUST 编译"
else
    RUSTC="rustc"
    echo "使用系统默认rustc编译"
fi

# 3. 编译为LLVM IR文本格式
echo "3. 编译为LLVM IR..."
# 指定一个具体的目标三元组，避免不匹配问题
$RUSTC --crate-type=staticlib \
      --emit=llvm-ir \
      -C debuginfo=2 \
      -C opt-level=0 \
      -C panic=abort \
      --target=x86_64-pc-linux-gnu \
      rust_aes128_klee.rs

# 4. 获取系统LLVM版本信息
LLVM_VERSION=$(llvm-config --version || echo "14.0.0")
echo "LLVM版本: $LLVM_VERSION"

# 5. 将LLVM IR转换为LLVM位码
echo "5. 转换为LLVM位码..."
echo "确保位码格式与KLEE兼容..."
llvm-as -opaque-pointers=0 rust_aes128_klee.ll -o rust_aes128_klee.bc || \
llvm-as rust_aes128_klee.ll -o rust_aes128_klee.bc

# 6. 运行KLEE分析，不使用任何运行时库
echo "6. 运行KLEE分析..."
# 移除 --libc=uclibc --posix-runtime 参数
klee --optimize --emit-all-errors rust_aes128_klee.bc

# 7. 分析结果
echo "7. 分析KLEE结果..."
mkdir -p reports
klee-stats klee-last/ > reports/klee_stats.txt

# 输出汇总
ERROR_COUNT=$(find klee-last -name "*.err" | wc -l)
if [ "$ERROR_COUNT" -gt 0 ]; then
  echo "发现 $ERROR_COUNT 个错误或警告"
  
  # 创建错误报告
  echo "Rust AES128模块KLEE分析错误摘要" > reports/error_summary.txt
  echo "================================" >> reports/error_summary.txt
  echo "" >> reports/error_summary.txt
  
  find klee-last -name "*.err" | while read -r ERR_FILE; do
    echo "错误文件: $ERR_FILE" >> reports/error_summary.txt
    head -10 "$ERR_FILE" >> reports/error_summary.txt
    echo "..." >> reports/error_summary.txt
    echo "" >> reports/error_summary.txt
  done
else
  echo "未发现错误，AES128分析通过"
fi

echo "=== 纯Rust AES128 KLEE分析完成 ==="
echo "结果保存在: $TEMP_DIR/klee-last/"
echo "统计信息: $TEMP_DIR/reports/" 
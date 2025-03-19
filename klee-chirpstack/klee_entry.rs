//! ChirpStack AES128模块的KLEE分析入口点
//! 
//! 这个文件作为纯Rust的KLEE分析入口点，直接测试模块，避免通过C FFI带来的开销
//! 
//! 编译命令：
//! RUSTFLAGS="--emit=llvm-ir -C debuginfo=2 -C opt-level=0 --cfg=feature=\"klee_analysis\"" \
//! rustc -C panic=abort --crate-type=lib klee_entry.rs -o klee_entry.bc

#![cfg(feature = "klee_analysis")]

extern crate lrwn;

use std::os::raw::c_void;

// 定义KLEE外部函数
extern "C" {
    fn klee_make_symbolic(addr: *mut c_void, size: usize, name: *const u8);
    fn klee_assume(condition: u8);
    fn klee_assert(condition: u8);
    fn klee_report_error(file: *const u8, line: u32, message: *const u8, suffix: *const u8);
}

// 封装KLEE函数，以便在Rust中使用
fn make_symbolic<T: Sized>(val: &mut T, name: &str) {
    unsafe {
        klee_make_symbolic(
            val as *mut T as *mut c_void,
            std::mem::size_of::<T>(),
            format!("{}\0", name).as_ptr(),
        );
    }
}

fn klee_assert_eq<T: PartialEq + std::fmt::Debug>(a: T, b: T, message: &str) {
    if a != b {
        unsafe {
            klee_report_error(
                b"klee_entry.rs\0".as_ptr(),
                123,
                format!("{:?} != {:?}: {}\0", a, b, message).as_ptr(),
                b"assertion_fail\0".as_ptr(),
            );
        }
    }
}

// 测试1: AES128加密和解密
fn test_aes128_encryption_decryption() {
    use lrwn::AES128Key;
    
    // 创建符号化密钥
    let mut key_bytes = [0u8; 16];
    make_symbolic(&mut key_bytes, "aes_key");
    let key = AES128Key::from_bytes(key_bytes);
    
    // 创建符号化数据
    let mut original_data = [0u8; 16];
    make_symbolic(&mut original_data, "plaintext");
    
    // 保存原始数据副本
    let original_data_copy = original_data;
    
    // 加密数据
    let mut encrypted_data = original_data;
    key.encrypt(&mut encrypted_data);
    
    // 检查是否有变化
    let mut has_changed = false;
    for i in 0..16 {
        if encrypted_data[i] != original_data_copy[i] {
            has_changed = true;
            break;
        }
    }
    
    unsafe {
        klee_assert(has_changed as u8);
    }
    
    // 解密数据
    key.decrypt(&mut encrypted_data);
    
    // 验证解密后与原始数据相同
    for i in 0..16 {
        klee_assert_eq(encrypted_data[i], original_data_copy[i], "解密后数据应该与原始数据相同");
    }
}

// 测试2: 不同密钥测试
fn test_different_keys() {
    use lrwn::AES128Key;
    
    // 创建两个符号化密钥
    let mut key1_bytes = [0u8; 16];
    let mut key2_bytes = [0u8; 16];
    make_symbolic(&mut key1_bytes, "key1");
    make_symbolic(&mut key2_bytes, "key2");
    
    let key1 = AES128Key::from_bytes(key1_bytes);
    let key2 = AES128Key::from_bytes(key2_bytes);
    
    // 创建符号化数据
    let mut data = [0u8; 16];
    make_symbolic(&mut data, "data_for_two_keys");
    
    // 使用两个密钥分别加密
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
    
    // 如果结果相同，检查密钥是否相同
    if results_equal {
        let mut keys_equal = true;
        for i in 0..16 {
            if key1_bytes[i] != key2_bytes[i] {
                keys_equal = false;
                break;
            }
        }
        
        unsafe {
            // 断言：如果加密结果相同，则密钥也必须相同
            klee_assert(keys_equal as u8);
        }
    }
}

// 测试3: 密钥敏感性分析
fn test_key_sensitivity() {
    use lrwn::AES128Key;
    
    // 创建符号化密钥
    let mut key_bytes = [0u8; 16];
    make_symbolic(&mut key_bytes, "base_key_for_sensitivity");
    
    // 创建修改后的密钥（翻转一个比特）
    let mut modified_key_bytes = key_bytes;
    
    // 选择要修改的字节和位
    let mut byte_index: u8 = 0;
    let mut bit_index: u8 = 0;
    make_symbolic(&mut byte_index, "byte_to_modify");
    make_symbolic(&mut bit_index, "bit_to_flip");
    
    // 限制有效范围
    unsafe {
        klee_assume(byte_index < 16);
        klee_assume(bit_index < 8);
    }
    
    // 翻转指定位
    modified_key_bytes[byte_index as usize] ^= 1 << bit_index;
    
    // 创建两个密钥实例
    let key1 = AES128Key::from_bytes(key_bytes);
    let key2 = AES128Key::from_bytes(modified_key_bytes);
    
    // 创建符号化数据
    let mut data = [0u8; 16];
    make_symbolic(&mut data, "data_for_sensitivity");
    
    // 使用两个密钥加密
    let mut result1 = data;
    let mut result2 = data;
    
    key1.encrypt(&mut result1);
    key2.encrypt(&mut result2);
    
    // 检查结果是否不同（一个好的加密算法应该对密钥的微小变化非常敏感）
    let mut results_different = false;
    for i in 0..16 {
        if result1[i] != result2[i] {
            results_different = true;
            break;
        }
    }
    
    unsafe {
        // 断言：修改密钥的一个位应该导致加密结果变化
        klee_assert(results_different as u8);
    }
}

// KLEE入口点
#[no_mangle]
pub extern "C" fn main() {
    // 运行所有测试
    test_aes128_encryption_decryption();
    test_different_keys();
    test_key_sensitivity();
} 
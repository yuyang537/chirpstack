/**
 * ChirpStack AES128模块KLEE分析驱动
 * 
 * 编译命令:
 *   clang -I /path/to/klee/include -emit-llvm -c -g -O0 aes128_driver.c -o aes128_driver.bc
 * 
 * KLEE执行:
 *   klee --libc=uclibc --posix-runtime aes128_driver.bc
 */

/* 使用包装头文件，避免依赖KLEE头文件位置 */
#include "klee_wrappers.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// 从Rust模块导出的函数声明
extern void aes128_encrypt(const unsigned char *key, unsigned char *data);
extern void aes128_decrypt(const unsigned char *key, unsigned char *data);
extern int test_aes_roundtrip(void);

// 测试1: 加密/解密循环
void test_encrypt_decrypt_cycle() {
    unsigned char key[16], data[16], encrypted[16];
    
    // 创建符号化输入
    klee_make_symbolic(key, sizeof(key), "aes_key");
    klee_make_symbolic(data, sizeof(data), "plaintext");
    
    // 保存原始数据以验证
    memcpy(encrypted, data, 16);
    
    // 加密数据
    aes128_encrypt(key, encrypted);
    
    // 确保加密改变了数据
    int changed = 0;
    for (int i = 0; i < 16; i++) {
        if (encrypted[i] != data[i]) {
            changed = 1;
            break;
        }
    }
    klee_assert(changed && "加密应该改变数据");
    
    // 解密数据
    aes128_decrypt(key, encrypted);
    
    // 验证解密后与原始数据相同
    for (int i = 0; i < 16; i++) {
        klee_assert(encrypted[i] == data[i] && "解密后应该恢复原始数据");
    }
}

// 测试2: 密钥相等性测试
void test_key_equivalence() {
    unsigned char key1[16], key2[16], data[16], result1[16], result2[16];
    
    // 创建符号化输入
    klee_make_symbolic(key1, sizeof(key1), "key1");
    klee_make_symbolic(key2, sizeof(key2), "key2");
    klee_make_symbolic(data, sizeof(data), "data");
    
    // 复制数据用于两次加密
    memcpy(result1, data, 16);
    memcpy(result2, data, 16);
    
    // 使用两个密钥加密
    aes128_encrypt(key1, result1);
    aes128_encrypt(key2, result2);
    
    // 检查在什么情况下两次加密结果相同
    int same_result = 1;
    for (int i = 0; i < 16; i++) {
        if (result1[i] != result2[i]) {
            same_result = 0;
            break;
        }
    }
    
    if (same_result) {
        // 如果结果相同，验证密钥是否也相同
        int same_key = 1;
        for (int i = 0; i < 16; i++) {
            if (key1[i] != key2[i]) {
                same_key = 0;
                break;
            }
        }
        klee_assert(same_key && "不同密钥不应该产生相同的加密结果");
    }
}

// 测试3: 密钥敏感性分析
void test_key_sensitivity() {
    unsigned char key[16], modified_key[16], data[16], result1[16], result2[16];
    
    // 创建符号化输入
    klee_make_symbolic(key, sizeof(key), "base_key");
    klee_make_symbolic(data, sizeof(data), "input_data");
    
    // 复制密钥和数据
    memcpy(modified_key, key, 16);
    memcpy(result1, data, 16);
    memcpy(result2, data, 16);
    
    // 修改密钥的一个比特
    int byte_to_change = 0;
    klee_make_symbolic(&byte_to_change, sizeof(byte_to_change), "byte_index");
    klee_assume(byte_to_change >= 0 && byte_to_change < 16);
    
    int bit_to_flip = 0;
    klee_make_symbolic(&bit_to_flip, sizeof(bit_to_flip), "bit_to_flip");
    klee_assume(bit_to_flip >= 0 && bit_to_flip < 8);
    
    modified_key[byte_to_change] ^= (1 << bit_to_flip);
    
    // 使用原始密钥和修改后的密钥加密
    aes128_encrypt(key, result1);
    aes128_encrypt(modified_key, result2);
    
    // 检查结果是否不同 (一个好的加密算法应该对密钥变化非常敏感)
    int different = 0;
    for (int i = 0; i < 16; i++) {
        if (result1[i] != result2[i]) {
            different = 1;
            break;
        }
    }
    
    klee_assert(different && "修改密钥一个比特应该产生不同的加密结果");
}

// 主入口函数
int main() {
    // 使用klee_assert验证Rust内部测试
    int roundtrip_result = test_aes_roundtrip();
    klee_assert(roundtrip_result && "Rust roundtrip测试失败");
    
    // 运行各个测试用例
    test_encrypt_decrypt_cycle();
    test_key_equivalence();
    test_key_sensitivity();
    
    return 0;
} 
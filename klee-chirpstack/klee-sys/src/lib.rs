//! 简单的KLEE系统绑定
//!
//! 这个包提供了KLEE符号执行引擎的基本Rust绑定

use std::os::raw::{c_char, c_void};

/// 将内存块标记为符号化
///
/// # 安全性
///
/// 这个函数是unsafe的，因为它直接操作内存
#[link(name = "kleeRuntest")]
extern "C" {
    pub fn klee_make_symbolic(addr: *mut c_void, size: usize, name: *const c_char);
    
    /// 添加假设条件
    pub fn klee_assume(condition: u8);
    
    /// 添加断言
    pub fn klee_assert(condition: u8);
    
    /// 报告错误
    pub fn klee_report_error(
        file: *const c_char,
        line: u32,
        message: *const c_char,
        suffix: *const c_char,
    );
    
    /// 打印表达式
    pub fn klee_print_expr(expr: u8, name: *const c_char);
    
    /// 生成测试用例
    pub fn klee_generate_test_case();
}

/// 创建符号化的内存块
///
/// 这是一个安全的包装函数，使用泛型简化符号化过程
pub fn make_symbolic<T: Sized>(val: &mut T, name: &str) {
    let c_name = format!("{}\0", name);
    unsafe {
        klee_make_symbolic(
            val as *mut T as *mut c_void,
            std::mem::size_of::<T>(),
            c_name.as_ptr() as *const c_char,
        );
    }
}
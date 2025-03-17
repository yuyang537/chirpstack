/**
 * KLEE符号执行引擎的Rust绑定
 * 
 * 本库提供了与KLEE符号执行引擎交互的接口，允许Rust代码使用KLEE进行符号执行分析。
 * 主要功能包括创建符号变量、添加约束和断言等。
 */

use std::os::raw::{c_char, c_int, c_void};

extern "C" {
    /// 将内存区域标记为符号变量
    /// 
    /// # 参数
    /// * `addr` - 要标记为符号的内存区域的起始地址
    /// * `size` - 内存区域的大小（字节）
    /// * `name` - 符号变量的名称，必须以null结尾的C字符串
    pub fn klee_make_symbolic(addr: *mut c_void, size: usize, name: *const c_char);

    /// 添加约束条件
    /// 
    /// # 参数
    /// * `condition` - 约束条件，非零值表示条件为真
    pub fn klee_assume(condition: c_int);

    /// 添加断言
    /// 
    /// # 参数
    /// * `condition` - 断言条件，非零值表示条件为真
    /// * `message` - 断言失败时的错误消息，必须以null结尾的C字符串
    pub fn klee_assert_with_message(condition: c_int, message: *const c_char);

    /// 添加断言（无错误消息版本）
    /// 
    /// # 参数
    /// * `condition` - 断言条件，非零值表示条件为真
    pub fn klee_assert(condition: c_int);

    /// 标记当前执行路径为错误
    /// 
    /// # 参数
    /// * `message` - 错误消息，必须以null结尾的C字符串
    pub fn klee_report_error(
        file: *const c_char,
        line: c_int,
        message: *const c_char,
        suffix: *const c_char,
    );

    /// 生成一个测试用例并终止当前执行路径
    pub fn klee_silent_exit(status: c_int);

    /// 获取符号值
    pub fn klee_get_value_i32(expr: c_int) -> c_int;
    pub fn klee_get_value_i64(expr: i64) -> i64;

    /// 打印表达式
    pub fn klee_print_expr(name: *const c_char, expr: c_int);
}

/// 创建符号变量的辅助函数
/// 
/// # 参数
/// * `name` - 符号变量的名称
/// 
/// # 返回值
/// 返回一个符号值
#[inline]
pub fn klee_int(name: &str) -> i32 {
    let mut result: i32 = 0;
    unsafe {
        let c_name = std::ffi::CString::new(name).unwrap();
        klee_make_symbolic(
            &mut result as *mut i32 as *mut c_void,
            std::mem::size_of::<i32>(),
            c_name.as_ptr(),
        );
    }
    result
}

/// 创建符号布尔值的辅助函数
/// 
/// # 参数
/// * `name` - 符号变量的名称
/// 
/// # 返回值
/// 返回一个符号布尔值
#[inline]
pub fn klee_bool(name: &str) -> bool {
    let mut result: bool = false;
    unsafe {
        let c_name = std::ffi::CString::new(name).unwrap();
        klee_make_symbolic(
            &mut result as *mut bool as *mut c_void,
            std::mem::size_of::<bool>(),
            c_name.as_ptr(),
        );
    }
    result
}

/// 添加断言并提供错误消息
/// 
/// # 参数
/// * `condition` - 断言条件
/// * `message` - 断言失败时的错误消息
#[inline]
pub fn klee_assert_msg(condition: bool, message: &str) {
    unsafe {
        let c_message = std::ffi::CString::new(message).unwrap();
        klee_assert_with_message(condition as c_int, c_message.as_ptr());
    }
}

/// 报告错误并终止当前执行路径
/// 
/// # 参数
/// * `file` - 文件名
/// * `line` - 行号
/// * `message` - 错误消息
/// * `suffix` - 错误后缀
#[inline]
pub fn report_error(file: &str, line: i32, message: &str, suffix: &str) {
    unsafe {
        let c_file = std::ffi::CString::new(file).unwrap();
        let c_message = std::ffi::CString::new(message).unwrap();
        let c_suffix = std::ffi::CString::new(suffix).unwrap();
        klee_report_error(c_file.as_ptr(), line, c_message.as_ptr(), c_suffix.as_ptr());
    }
} 
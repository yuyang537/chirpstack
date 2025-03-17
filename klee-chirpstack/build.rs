/**
 * KLEE-sys构建脚本
 * 
 * 本脚本用于编译KLEE的C接口，并将其链接到Rust库中。
 * 在实际使用KLEE时，这些函数会被KLEE工具替换为其内部实现。
 */

fn main() {
    // 当在KLEE环境中运行时，这些函数会被KLEE工具替换
    // 这里我们提供一个空实现，以便在非KLEE环境中编译通过
    let src = r#"
    #include <stddef.h>
    #include <stdint.h>

    // 将内存区域标记为符号变量
    void klee_make_symbolic(void *addr, size_t size, const char *name) {
        // 在非KLEE环境中，这是一个空操作
        (void)addr;
        (void)size;
        (void)name;
    }

    // 添加约束条件
    void klee_assume(int condition) {
        // 在非KLEE环境中，这是一个空操作
        (void)condition;
    }

    // 添加断言
    void klee_assert(int condition) {
        // 在非KLEE环境中，这是一个空操作
        (void)condition;
    }

    // 添加带消息的断言
    void klee_assert_with_message(int condition, const char *message) {
        // 在非KLEE环境中，这是一个空操作
        (void)condition;
        (void)message;
    }

    // 报告错误
    void klee_report_error(const char *file, int line, const char *message, const char *suffix) {
        // 在非KLEE环境中，这是一个空操作
        (void)file;
        (void)line;
        (void)message;
        (void)suffix;
    }

    // 静默退出
    void klee_silent_exit(int status) {
        // 在非KLEE环境中，这是一个空操作
        (void)status;
    }

    // 获取符号值
    int klee_get_value_i32(int expr) {
        return expr;
    }

    int64_t klee_get_value_i64(int64_t expr) {
        return expr;
    }

    // 打印表达式
    void klee_print_expr(const char *name, int expr) {
        // 在非KLEE环境中，这是一个空操作
        (void)name;
        (void)expr;
    }
    "#;

    // 编译C代码
    cc::Build::new()
        .file("src/klee_stubs.c")
        .compile("klee_stubs");

    // 将C代码写入文件
    std::fs::write("src/klee_stubs.c", src).unwrap();

    // 重新运行cargo，如果C文件发生变化
    println!("cargo:rerun-if-changed=src/klee_stubs.c");
} 
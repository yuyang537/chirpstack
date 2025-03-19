/**
 * KLEE函数包装头文件
 * 
 * 当找不到KLEE原始头文件时使用此文件
 */

#ifndef KLEE_WRAPPERS_H
#define KLEE_WRAPPERS_H

#include <stddef.h>

// 声明KLEE函数
void klee_make_symbolic(void *addr, size_t nbytes, const char *name);
void klee_assume(int condition);
void klee_assert(int condition);
void klee_print_expr(int expr, const char *name);
void klee_report_error(const char *file, int line, const char *message, const char *suffix);

#endif /* KLEE_WRAPPERS_H */
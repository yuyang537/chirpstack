/**
 * @module helpers/errors
 * 
 * @description
 * 
 * # 模块概述
 * 本模块提供了错误处理的辅助功能，特别是用于格式化和打印详细错误信息。
 * 在ChirpStack系统中，错误处理和错误信息的可读性对于调试和问题排查至关重要。
 * 
 * # 文件功能
 * - 定义PrintFullError特性，用于获取完整的错误信息
 * - 为系统中的各种错误类型实现该特性
 * - 提供统一的错误格式化方式
 * 
 * # 主要组件
 * - PrintFullError trait：定义获取完整错误信息的方法
 * - 为anyhow::Error实现PrintFullError
 * - 为storage::error::Error实现PrintFullError
 * - 为api::auth::error::Error实现PrintFullError
 * - 为lrwn::Error实现PrintFullError
 * 
 * # 关键流程
 * - 捕获错误后，调用full()方法获取详细错误信息
 * - 将错误信息格式化为人类可读的形式
 * - 在日志或调试输出中使用格式化的错误信息
 * 
 * # 重要考虑事项
 * - 错误信息应该包含足够的上下文以便于调试
 * - 错误格式化应该一致且易于理解
 * - 敏感信息不应在错误消息中暴露
 * - 错误处理应该优雅且不影响系统稳定性
 */

pub trait PrintFullError {
    fn full(&self) -> String;
}

impl PrintFullError for anyhow::Error {
    fn full(&self) -> String {
        format!("{:#}", self)
    }
}

impl PrintFullError for crate::storage::error::Error {
    fn full(&self) -> String {
        format!("{:#}", self)
    }
}

impl PrintFullError for crate::api::auth::error::Error {
    fn full(&self) -> String {
        format!("{:#}", self)
    }
}

impl PrintFullError for lrwn::Error {
    fn full(&self) -> String {
        format!("{:#}", self)
    }
}

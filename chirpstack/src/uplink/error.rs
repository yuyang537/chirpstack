/**
 * @module uplink/error
 * 
 * @description
 * 
 * # 模块概述
 * 本模块定义了ChirpStack系统中上行数据处理过程中可能遇到的错误类型。它提供了一个统一的错误处理
 * 机制，使得上行数据处理模块可以以一致的方式处理和报告错误。这些错误类型涵盖了从处理流程中的
 * 正常中止到特定功能限制的各种情况。
 * 
 * # 文件功能
 * - 定义上行数据处理过程中的错误类型
 * - 提供错误类型之间的转换机制
 * - 为错误提供人类可读的描述
 * - 支持错误的调试输出
 * 
 * # 主要组件
 * - Error枚举：定义了上行数据处理中的各种错误类型
 *   - Abort：表示处理流程正常中止，无需进一步处理
 *   - RoamingIsNotAllowed：表示设备不允许漫游
 *   - Anyhow：包装通用错误类型，支持从其他错误类型转换
 * 
 * # 关键流程
 * - 错误处理流程：
 *   1. 在上行数据处理过程中检测到错误条件
 *   2. 创建相应的Error枚举实例
 *   3. 将错误返回给调用者或转换为其他错误类型
 *   4. 调用者根据错误类型采取相应的处理措施
 * 
 * # 重要考虑事项
 * - 错误类型应该提供足够的信息，以便于调试和问题排查
 * - 错误消息应该清晰明了，帮助理解错误的原因
 * - 错误处理应该考虑到不同的使用场景和上下文
 * - 随着系统功能的扩展，可能需要添加新的错误类型
 * - 错误类型的设计应该考虑到与其他模块的交互
 */

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Nothing else to do")]
    Abort,

    #[error("Roaming is not allowed for the device")]
    RoamingIsNotAllowed,

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

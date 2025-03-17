/*
 * 模块概述
 * ========
 * 错误模块是ChirpStack存储层的重要组成部分，定义了存储操作中可能遇到的各种错误类型。
 * 该模块通过统一的错误处理机制，使应用程序能够以一致的方式处理来自不同存储后端（如数据库、
 * Redis等）的错误，同时提供有意义的错误信息，便于调试和问题排查。
 * 
 * 错误模块采用了Rust的thiserror库，使错误定义更加简洁和表达力强，同时保持了良好的错误传播
 * 能力。通过实现标准的Error特性，这些错误可以与Rust生态系统中的其他错误处理机制无缝集成。
 *
 * 文件功能
 * ========
 * 本文件(error.rs)定义了存储模块的错误类型和错误处理功能，提供了以下主要功能：
 * 1. 定义存储操作相关的错误枚举类型
 * 2. 为每种错误类型提供有意义的错误消息
 * 3. 实现从其他错误类型（如Diesel错误、Redis错误等）到存储错误的转换
 * 4. 提供辅助方法，简化错误处理和错误消息生成
 *
 * 主要组件
 * ========
 * - Error枚举: 定义了存储模块中可能出现的所有错误类型
 *   - NotFound: 请求的对象不存在
 *   - AlreadyExists: 尝试创建的对象已经存在
 *   - InvalidEmail: 电子邮件格式无效
 *   - HashPassword: 密码哈希过程中的错误
 *   - InvalidUsernameOrPassword: 用户名或密码无效
 *   - InvalidPayload: 无效的数据负载
 *   - InvalidMIC: 消息完整性码(MIC)无效
 *   - InvalidDevNonce: 设备随机数无效
 *   - Validation: 数据验证错误
 *   - NotAllowed: 操作不被允许
 *   - 以及从其他错误类型转换而来的错误
 *
 * - from_diesel方法: 将Diesel ORM错误转换为存储错误，并添加上下文信息
 *
 * 关键流程
 * ========
 * 1. 错误生成流程:
 *    - 在存储操作中检测到错误条件
 *    - 创建适当的Error枚举变体实例
 *    - 添加相关上下文信息（如对象ID、错误描述等）
 *    - 返回错误或使用?运算符传播错误
 *
 * 2. 错误转换流程:
 *    - 捕获底层库（如Diesel、Redis）产生的错误
 *    - 使用From特性或专用方法将其转换为存储Error类型
 *    - 添加额外的上下文信息，使错误更有意义
 *
 * 注意事项
 * ========
 * - 错误消息: 错误消息应该清晰、具体，包含足够的上下文信息
 * - 错误传播: 使用?运算符简化错误传播
 * - 错误转换: 实现From特性，简化从其他错误类型的转换
 * - 错误处理: 在适当的层次处理错误，避免错误信息丢失
 * - 安全考虑: 避免在错误消息中泄露敏感信息
 */

use diesel::result::Error as ResultError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Object does not exist (id: {0})")]
    NotFound(String),

    #[error("Object already exists (id: {0})")]
    AlreadyExists(String),

    #[error("Invalid email address format")]
    InvalidEmail,

    #[error("Hash password error (error: {0})")]
    HashPassword(String),

    #[error("Invalid username or password")]
    InvalidUsernameOrPassword,

    #[error("Invalid type (expected: {0})")]
    InvalidPayload(String),

    #[error("Invalid MIC")]
    InvalidMIC,

    #[error("Invalid DevNonce")]
    InvalidDevNonce,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not allowed ({0})")]
    NotAllowed(String),

    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    Lrwn(#[from] lrwn::Error),

    #[error(transparent)]
    TokioJoin(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Redis(#[from] redis::RedisError),

    #[error(transparent)]
    ProstDecode(#[from] prost::DecodeError),
}

impl Error {
    pub fn from_diesel(e: diesel::result::Error, s: String) -> Self {
        match &e {
            ResultError::NotFound => Error::NotFound(s),
            _ => Error::Diesel(e),
        }
    }
}

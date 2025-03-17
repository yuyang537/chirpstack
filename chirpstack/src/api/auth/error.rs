/*
 * 模块概述
 * ========
 * 认证(Auth)模块是ChirpStack LoRaWAN网络服务器的核心安全组件，负责实现API访问的身份验证和授权机制。
 * 该模块确保只有经过身份验证的用户和应用程序才能访问ChirpStack的API接口，并根据其权限级别限制对特定资源的访问。
 * 本模块实现了基于JWT(JSON Web Token)的认证系统，支持用户账户和API密钥两种认证方式，为ChirpStack提供了灵活而安全的访问控制机制。
 * 
 * 认证模块与ChirpStack的其他组件紧密集成，为整个系统提供统一的安全层，保护敏感的网络配置和设备数据。
 *
 * 文件功能
 * ========
 * 本文件(error.rs)定义了认证模块中使用的错误类型和错误处理机制。
 * 它提供了一个统一的Error枚举类型，封装了认证过程中可能遇到的各种错误，包括通用错误、数据库错误和异步任务错误。
 * 这种统一的错误处理方式简化了错误传播和处理，提高了代码的可维护性和可读性。
 *
 * 主要组件
 * ========
 * - Error: 认证模块的错误枚举类型，包含以下变体：
 *   - Anyhow: 通用错误，封装anyhow::Error
 *   - Diesel: 数据库错误，封装diesel::result::Error
 *   - Join: 异步任务错误，封装tokio::task::JoinError
 *
 * 关键流程
 * ========
 * 1. 认证或授权过程中遇到错误时，创建相应的Error变体
 * 2. 使用thiserror的派生宏自动实现错误的Display特性
 * 3. 通过From特性实现，支持从原始错误类型自动转换为Error类型
 * 4. 在认证模块的函数中返回Result<T, Error>类型，统一错误处理
 *
 * 注意事项
 * ========
 * - 该错误类型主要用于validator子模块中的权限验证逻辑
 * - Error实现了标准的Error特性，可以与其他错误处理机制无缝集成
 * - 使用transparent属性，保留原始错误的错误信息和上下文
 * - 在API层面，这些错误最终会被转换为适当的gRPC状态码返回给客户端
 * - 数据库错误通常表示权限验证查询失败，可能需要检查数据库连接或表结构
 * - 异步任务错误通常表示权限验证过程中的并发问题
 */

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

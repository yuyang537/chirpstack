/*
 * 模块概述
 * ========
 * API错误处理模块是ChirpStack LoRaWAN网络服务器的核心基础组件，负责统一管理和转换系统中的各类错误。
 * 该模块在ChirpStack的API层与底层存储、网络和业务逻辑之间架起了桥梁，确保错误信息能够以一致且
 * 友好的方式传递给API客户端。
 * 
 * 在LoRaWAN网络服务器中，错误处理是保证系统稳定性和可靠性的关键部分。本模块通过将各种内部错误
 * 映射为标准的gRPC状态码，使客户端能够正确理解和处理错误情况，同时保持API接口的一致性。
 *
 * 文件功能
 * ========
 * 本文件(error.rs)实现了错误类型到gRPC状态码的转换机制，提供了统一的错误处理接口。
 * 它定义了ToStatus特性，允许各种错误类型转换为tonic::Status对象，这是gRPC响应中表示错误的标准方式。
 * 文件为ChirpStack中常见的错误类型实现了ToStatus特性，包括存储错误、UUID解析错误、LoRaWAN协议错误等。
 *
 * 主要组件
 * ========
 * - ToStatus: 核心特性，定义了将错误转换为gRPC状态的方法
 * - 各种错误类型的ToStatus实现:
 *   - storage::error::Error: 存储层错误转换
 *   - anyhow::Error: 通用错误转换
 *   - uuid::Error: UUID解析错误转换
 *   - lrwn::Error: LoRaWAN协议错误转换
 *   - Box<dyn std::error::Error>: 动态错误转换
 *   - tokio::task::JoinError: 异步任务错误转换
 *   - prost_types::TimestampError: 时间戳错误转换
 *   - std::num::ParseIntError: 整数解析错误转换
 *
 * 关键流程
 * ========
 * 1. API处理函数捕获内部错误（如数据库操作、协议解析等）
 * 2. 通过调用错误对象的status()方法，将错误转换为适当的gRPC状态
 * 3. 根据错误类型选择合适的状态码(Code)，如NotFound、AlreadyExists、InvalidArgument等
 * 4. 生成包含详细错误信息的Status对象
 * 5. 将Status对象返回给API客户端，提供清晰的错误上下文
 *
 * 注意事项
 * ========
 * - 错误转换应保持一致性，相同类型的错误应映射到相同的状态码
 * - 安全敏感的错误信息应适当过滤，避免泄露系统内部细节
 * - 存储错误的转换尤为重要，需要区分"资源不存在"和"服务器内部错误"等情况
 * - 对于认证相关错误，应使用Unauthenticated状态码而非其他错误类型
 * - 错误消息应提供足够的上下文信息，帮助客户端理解和解决问题
 * - 使用format!("{:#}", self)格式化错误时会包含错误链，提供更完整的错误上下文
 */

use tonic::{Code, Status};

use crate::storage;

pub trait ToStatus {
    fn status(&self) -> Status;
}

impl ToStatus for storage::error::Error {
    fn status(&self) -> Status {
        match self {
            storage::error::Error::NotFound(_) => Status::new(Code::NotFound, format!("{}", self)),
            storage::error::Error::AlreadyExists(_) => {
                Status::new(Code::AlreadyExists, format!("{:#}", self))
            }
            storage::error::Error::InvalidEmail => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::HashPassword(_) => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::InvalidUsernameOrPassword => {
                Status::new(Code::Unauthenticated, format!("{:#}", self))
            }
            storage::error::Error::InvalidPayload(_) => {
                Status::new(Code::Internal, format!("{:#}", self))
            }
            storage::error::Error::InvalidMIC => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::InvalidDevNonce => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::Validation(_) => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::NotAllowed(_) => {
                Status::new(Code::InvalidArgument, format!("{:#}", self))
            }
            storage::error::Error::Diesel(_) => Status::new(Code::Internal, format!("{:#}", self)),
            storage::error::Error::Anyhow(_) => Status::new(Code::Internal, format!("{:#}", self)),
            storage::error::Error::Lrwn(_) => Status::new(Code::Internal, format!("{:#}", self)),
            storage::error::Error::TokioJoin(_) => {
                Status::new(Code::Internal, format!("{:#}", self))
            }
            storage::error::Error::Redis(_) => Status::new(Code::Internal, format!("{:#}", self)),
            storage::error::Error::ProstDecode(_) => {
                Status::new(Code::Internal, format!("{:#}", self))
            }
        }
    }
}

impl ToStatus for anyhow::Error {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

impl ToStatus for uuid::Error {
    fn status(&self) -> Status {
        Status::new(Code::InvalidArgument, format!("{:#}", self))
    }
}

impl ToStatus for lrwn::Error {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

impl ToStatus for Box<dyn std::error::Error> {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

impl ToStatus for tokio::task::JoinError {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

impl ToStatus for prost_types::TimestampError {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

impl ToStatus for std::num::ParseIntError {
    fn status(&self) -> Status {
        Status::new(Code::Internal, format!("{:#}", self))
    }
}

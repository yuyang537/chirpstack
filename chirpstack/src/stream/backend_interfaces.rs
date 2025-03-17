/*
 * 模块概述
 * ========
 * 后端接口(Backend Interfaces)模块是ChirpStack LoRaWAN网络服务器流处理框架的重要组件，
 * 负责记录和管理与LoRaWAN后端接口相关的通信日志。该模块捕获ChirpStack与其他LoRaWAN网络
 * 服务器之间的通信，特别是在漫游场景下的服务器间消息交换。
 * 
 * 该模块实现了LoRaWAN Backend Interfaces规范中定义的通信日志记录，支持记录诸如漫游请求、
 * 加入请求转发、位置解析等后端接口通信。通过这些日志，系统管理员可以监控服务器间通信的
 * 状态，排查漫游问题，并优化网络性能。
 * 
 * 后端接口模块利用Redis流(Redis Streams)作为存储机制，并使用Tokio的异步通道(mpsc)实现
 * 高效的日志记录，确保日志记录不会阻塞主要业务流程。
 *
 * 文件功能
 * ========
 * 本文件(backend_interfaces.rs)实现了后端接口通信日志的记录功能，提供了以下主要功能：
 * 1. 创建日志发送器，用于异步记录后端接口通信日志
 * 2. 记录后端接口请求日志，包括请求类型、目标服务器和请求内容
 * 3. 根据配置限制后端接口日志的历史记录数量
 *
 * 主要组件
 * ========
 * - get_log_sender(): 创建并返回用于发送日志的异步通道发送端
 * - log_request(): 记录后端接口请求日志的主要函数
 *
 * 关键流程
 * ========
 * 1. 日志发送器创建流程:
 *    - 检查配置的历史记录限制
 *    - 创建异步通道(mpsc)的发送端和接收端
 *    - 启动后台任务处理接收到的日志请求
 *    - 返回发送端供其他模块使用
 *
 * 2. 后端接口日志记录流程:
 *    - 接收后端接口请求日志对象(BackendInterfacesRequest)
 *    - 检查配置的历史记录限制
 *    - 将请求日志编码并添加到Redis流中
 *    - 应用配置的历史记录限制
 *
 * 注意事项
 * ========
 * - 性能考虑: 使用异步通道确保日志记录不会阻塞主要业务流程
 * - 错误处理: 日志记录错误应被适当捕获和记录，但不应影响主要业务流程
 * - 存储优化: 合理配置历史记录限制，防止过度消耗存储资源
 * - 安全性: 后端接口日志可能包含敏感信息，需要适当保护
 * - 扩展性: 日志结构应具有良好的扩展性，以适应未来的监控需求
 */

use anyhow::Result;
use prost::Message;
use tokio::sync::mpsc::{self, Sender};
use tracing::error;

use crate::config;
use crate::storage::{get_async_redis_conn, redis_key};
use chirpstack_api::stream;

pub async fn get_log_sender() -> Option<Sender<stream::BackendInterfacesRequest>> {
    let conf = config::get();
    if conf.monitoring.backend_interfaces_log_max_history == 0 {
        return None;
    }

    let (tx, mut rx) = mpsc::channel(100);

    tokio::spawn(async move {
        while let Some(pl) = rx.recv().await {
            tokio::spawn(async move {
                if let Err(e) = log_request(pl).await {
                    error!(error = %e, "Log request error");
                }
            });
        }
    });

    Some(tx)
}

pub async fn log_request(pl: stream::BackendInterfacesRequest) -> Result<()> {
    let conf = config::get();

    if conf.monitoring.backend_interfaces_log_max_history == 0 {
        return Ok(());
    }

    let key = redis_key("backend_interfaces:stream:request".to_string());
    let b = pl.encode_to_vec();
    () = redis::cmd("XADD")
        .arg(&key)
        .arg("MAXLEN")
        .arg(conf.monitoring.backend_interfaces_log_max_history)
        .arg("*")
        .arg("request")
        .arg(&b)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    Ok(())
}

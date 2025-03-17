/*
 * 模块概述
 * ========
 * API请求(API Request)模块是ChirpStack LoRaWAN网络服务器流处理框架的重要组件，负责记录
 * 和管理系统API的请求日志。该模块捕获所有通过gRPC接口发送到ChirpStack的API请求，提供了
 * 完整的API使用记录，对于系统监控、安全审计和性能分析至关重要。
 * 
 * 该模块利用Redis流(Redis Streams)作为存储机制，记录API请求的关键信息，包括服务名称、
 * 方法名称和元数据等。通过这些日志，系统管理员可以监控API的使用情况，识别潜在的安全
 * 问题，并优化API性能。
 * 
 * API请求模块是ChirpStack安全框架的重要组成部分，为系统提供了必要的审计和监控能力，
 * 同时也为API使用分析和优化提供了数据支持。
 *
 * 文件功能
 * ========
 * 本文件(api_request.rs)实现了API请求日志的记录功能，提供了以下主要功能：
 * 1. 记录API请求日志，包括服务名称、方法名称和元数据
 * 2. 根据配置限制API请求日志的历史记录数量
 * 3. 提供测试功能，验证API请求日志的记录和查询
 *
 * 主要组件
 * ========
 * - log_request(): 记录API请求日志的主要函数
 * - tests模块: 包含用于测试API请求日志功能的测试用例
 *
 * 关键流程
 * ========
 * 1. API请求日志记录流程:
 *    - 接收API请求日志对象(ApiRequestLog)
 *    - 检查配置的历史记录限制
 *    - 将请求日志编码并添加到Redis流中
 *    - 应用配置的历史记录限制
 *
 * 2. 测试流程:
 *    - 创建测试API请求日志对象
 *    - 调用log_request函数记录日志
 *    - 从Redis流中读取记录的日志
 *    - 验证日志内容是否与原始对象一致
 *
 * 注意事项
 * ========
 * - 性能考虑: API请求日志记录应尽量轻量化，避免影响API响应时间
 * - 存储优化: 合理配置历史记录限制，防止过度消耗存储资源
 * - 安全性: API请求日志可能包含敏感信息，需要适当保护
 * - 隐私合规: 确保日志记录符合相关的隐私法规和政策
 * - 扩展性: 日志结构应具有良好的扩展性，以适应未来的监控需求
 */

use anyhow::Result;
use prost::Message;

use crate::config;
use crate::storage::{get_async_redis_conn, redis_key};
use chirpstack_api::stream;

pub async fn log_request(pl: &stream::ApiRequestLog) -> Result<()> {
    let conf = config::get();

    if conf.monitoring.api_request_log_max_history == 0 {
        return Ok(());
    }

    let key = redis_key("api:stream:request".to_string());
    let b = pl.encode_to_vec();
    () = redis::cmd("XADD")
        .arg(&key)
        .arg("MAXLEN")
        .arg(conf.monitoring.api_request_log_max_history)
        .arg("*")
        .arg("request")
        .arg(&b)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test;
    use redis::streams::StreamReadReply;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_log_request() {
        let _guard = test::prepare().await;

        let pl = stream::ApiRequestLog {
            service: "ap.Foo".to_string(),
            method: "bar".to_string(),
            metadata: [("user_id".to_string(), "foo_user".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
        log_request(&pl).await.unwrap();

        let key = redis_key("api:stream:request".to_string());
        let srr: StreamReadReply = redis::cmd("XREAD")
            .arg("COUNT")
            .arg(1_usize)
            .arg("STREAMS")
            .arg(&key)
            .arg("0")
            .query_async(&mut get_async_redis_conn().await.unwrap())
            .await
            .unwrap();

        assert_eq!(1, srr.keys.len());
        assert_eq!(1, srr.keys[0].ids.len());

        if let Some(redis::Value::BulkString(b)) = srr.keys[0].ids[0].map.get("request") {
            let pl_recv = stream::ApiRequestLog::decode(&mut Cursor::new(b)).unwrap();
            assert_eq!(pl, pl_recv);
        } else {
            panic!("No request log");
        }
    }
}

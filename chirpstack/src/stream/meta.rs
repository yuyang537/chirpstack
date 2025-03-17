/*
 * 模块概述
 * ========
 * 元数据(Meta)模块是ChirpStack LoRaWAN网络服务器流处理框架的重要组件，负责记录和管理
 * 系统运行的元数据信息。元数据是指描述系统状态、性能和操作的高级信息，与具体的业务数据
 * 相对独立，但对于系统监控、性能分析和故障诊断至关重要。
 * 
 * 该模块利用Redis流(Redis Streams)作为存储机制，记录上行和下行通信的元数据，包括处理
 * 时间、资源使用情况、队列状态等关键指标。通过这些元数据，系统管理员和开发人员可以全面
 * 了解ChirpStack的运行状况，及时发现潜在问题。
 * 
 * 元数据模块与其他流处理模块协同工作，但专注于系统级别的信息收集，为整个ChirpStack
 * 平台提供了重要的监控和分析能力。
 *
 * 文件功能
 * ========
 * 本文件(meta.rs)实现了元数据的记录功能，提供了以下主要功能：
 * 1. 记录上行通信元数据，包括处理时间、资源使用等信息
 * 2. 记录下行通信元数据，包括队列状态、发送延迟等信息
 * 3. 根据配置限制元数据历史记录数量，优化存储使用
 *
 * 主要组件
 * ========
 * - log_uplink(): 记录上行通信元数据
 * - log_downlink(): 记录下行通信元数据
 *
 * 关键流程
 * ========
 * 1. 上行元数据记录流程:
 *    - 接收上行元数据对象(UplinkMeta)
 *    - 检查配置的历史记录限制
 *    - 将元数据编码并添加到Redis流中
 *    - 应用配置的历史记录限制
 *
 * 2. 下行元数据记录流程:
 *    - 接收下行元数据对象(DownlinkMeta)
 *    - 检查配置的历史记录限制
 *    - 将元数据编码并添加到Redis流中
 *    - 应用配置的历史记录限制
 *
 * 注意事项
 * ========
 * - 性能影响: 元数据记录应尽量轻量化，避免影响主要业务流程
 * - 存储优化: 合理配置历史记录限制，防止过度消耗存储资源
 * - 数据价值: 元数据对系统监控和故障诊断具有重要价值，应确保其完整性
 * - 隐私考虑: 元数据可能包含敏感信息，需要适当保护
 * - 扩展性: 元数据结构应具有良好的扩展性，以适应未来的监控需求
 */

use anyhow::Result;
use prost::Message;

use crate::config;
use crate::storage::{get_async_redis_conn, redis_key};
use chirpstack_api::stream;

pub async fn log_uplink(up: &stream::UplinkMeta) -> Result<()> {
    let conf = config::get();

    if conf.monitoring.meta_log_max_history > 0 {
        let key = redis_key("stream:meta".to_string());
        let b = up.encode_to_vec();
        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.meta_log_max_history)
            .arg("*")
            .arg("up")
            .arg(&b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

pub async fn log_downlink(down: &stream::DownlinkMeta) -> Result<()> {
    let conf = config::get();

    if conf.monitoring.meta_log_max_history > 0 {
        let key = redis_key("stream:meta".to_string());
        let b = down.encode_to_vec();

        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.meta_log_max_history)
            .arg("*")
            .arg("down")
            .arg(&b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

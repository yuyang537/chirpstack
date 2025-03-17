/*
 * 模块概述
 * ========
 * downlink_frame模块提供了用于管理LoRaWAN下行帧数据的临时存储和检索功能。下行帧包含了
 * 网络服务器发送给网关的下行消息数据，包括PHY载荷、发送时间、频率等参数。这些信息在
 * 下行消息的调度和确认过程中起着关键作用。
 * 
 * 该模块使用Redis作为存储后端，将下行帧数据序列化为Protocol Buffers格式存储。这种方式
 * 提供了高效的数据访问能力，同时支持短期的数据保留策略。下行帧数据通过唯一的下行ID进行
 * 索引，确保每个下行消息能够被准确地跟踪和管理。
 *
 * 文件功能
 * ========
 * 本文件(downlink_frame.rs)实现了下行帧数据的存储和检索功能，提供了以下主要功能：
 * 1. 保存下行帧数据到Redis中，设置短期的过期时间
 * 2. 根据下行ID获取并删除下行帧数据（一次性读取）
 * 3. 处理下行帧数据的序列化和反序列化
 *
 * 主要组件
 * ========
 * - save函数: 将下行帧数据保存到Redis中，设置30秒的过期时间
 * - get_and_del函数: 从Redis中获取并删除下行帧数据
 * - Protocol Buffers序列化/反序列化: 用于高效地编码和解码下行帧数据
 *
 * 关键流程
 * ========
 * 1. 下行帧保存流程:
 *    - 使用Protocol Buffers将DownlinkFrame结构编码为二进制数据
 *    - 根据下行ID构造Redis键
 *    - 将序列化数据保存到Redis中，设置30秒的过期时间
 *    - 记录日志，表明下行帧已保存
 *
 * 2. 下行帧检索流程:
 *    - 根据下行ID构造Redis键
 *    - 从Redis获取并删除序列化的下行帧数据（原子操作）
 *    - 使用Protocol Buffers将二进制数据解码为DownlinkFrame结构
 *    - 返回下行帧数据或NotFound错误
 *
 * 注意事项
 * ========
 * - 短期存储: 下行帧数据仅保留30秒，适用于临时跟踪和确认
 * - 一次性读取: get_and_del函数同时获取并删除数据，确保每个下行帧只被处理一次
 * - 性能考虑: Redis提供高性能的数据访问，适合高吞吐量的下行消息处理
 * - 错误处理: 适当处理Redis连接和数据序列化过程中的错误
 * - 测试支持: 包含测试模块，用于验证下行帧存储和检索功能
 */

use std::io::Cursor;

use anyhow::Result;
use prost::Message;
use tracing::info;

use super::{error::Error, get_async_redis_conn, redis_key};
use chirpstack_api::internal;

pub async fn save(df: &internal::DownlinkFrame) -> Result<()> {
    let b = df.encode_to_vec();
    let key = redis_key(format!("frame:{}", df.downlink_id));

    () = redis::cmd("SETEX")
        .arg(key)
        .arg(30)
        .arg(b)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(downlink_id = df.downlink_id, "Downlink-frame saved");
    Ok(())
}

pub async fn get_and_del(id: u32) -> Result<internal::DownlinkFrame, Error> {
    let key = redis_key(format!("frame:{}", id));
    let v: Vec<u8> = redis::cmd("GETDEL")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;
    if v.is_empty() {
        return Err(Error::NotFound(format!("{}", id)));
    }
    let df = internal::DownlinkFrame::decode(&mut Cursor::new(v))?;
    Ok(df)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::test;
    use chirpstack_api::gw;

    #[tokio::test]
    async fn test_downlink_frame() {
        let _guard = test::prepare().await;
        let df = internal::DownlinkFrame {
            downlink_id: 12345,
            downlink_frame: Some(gw::DownlinkFrame {
                ..Default::default()
            }),
            ..Default::default()
        };

        save(&df).await.unwrap();
        let df_get = get_and_del(12345).await.unwrap();
        assert_eq!(df, df_get);
    }
}

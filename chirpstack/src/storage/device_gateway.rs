/*
 * 模块概述
 * ========
 * device_gateway模块提供了用于管理设备与网关之间关联信息的存储和检索功能。该模块主要
 * 处理设备上行消息的接收信息(RX Info)，包括接收信号强度、信噪比、接收时间等关键参数。
 * 这些信息对于下行消息的调度、网关选择和信号质量分析至关重要。
 * 
 * 该模块使用Redis作为存储后端，将设备网关接收信息序列化为Protocol Buffers格式存储。
 * 这种方式提供了高效的数据访问能力，同时支持临时的数据保留策略。设备网关数据通过设备的
 * EUI(扩展唯一标识符)进行索引，确保每个设备的网关接收信息能够被快速检索。
 *
 * 文件功能
 * ========
 * 本文件(device_gateway.rs)实现了设备网关接收信息的存储和检索功能，提供了以下主要功能：
 * 1. 保存设备的网关接收信息
 * 2. 根据设备EUI获取网关接收信息
 * 3. 批量获取多个设备的网关接收信息
 * 4. 处理网关接收信息的序列化和反序列化
 *
 * 主要组件
 * ========
 * - save_rx_info函数: 将设备的网关接收信息保存到Redis中
 * - get_rx_info函数: 根据设备EUI获取网关接收信息
 * - get_rx_info_for_dev_euis函数: 批量获取多个设备的网关接收信息
 * - Protocol Buffers序列化/反序列化: 用于高效地编码和解码网关接收信息
 *
 * 关键流程
 * ========
 * 1. 网关接收信息保存流程:
 *    - 从设备上行消息中提取网关接收信息
 *    - 使用Protocol Buffers将DeviceGatewayRxInfo结构编码为二进制数据
 *    - 根据设备EUI构造Redis键
 *    - 将序列化数据保存到Redis中，设置与设备会话相同的过期时间
 *    - 记录日志，表明网关接收信息已保存
 *
 * 2. 网关接收信息检索流程:
 *    - 根据设备EUI构造Redis键
 *    - 从Redis获取序列化的网关接收信息
 *    - 使用Protocol Buffers将二进制数据解码为DeviceGatewayRxInfo结构
 *    - 返回网关接收信息或NotFound错误
 *
 * 注意事项
 * ========
 * - 数据时效性: 网关接收信息具有时效性，主要用于短期的下行消息调度
 * - 过期策略: 使用与设备会话相同的过期时间，防止资源泄漏
 * - 性能考虑: Redis提供高性能的数据访问，适合频繁的网关信息更新
 * - 批量操作: 支持批量获取多个设备的网关信息，提高效率
 * - 错误处理: 适当处理Redis连接和数据序列化过程中的错误
 */

use std::io::Cursor;

use anyhow::{Context, Result};
use prost::Message;
use tracing::info;

use super::{error::Error, get_async_redis_conn, redis_key};
use crate::config;
use chirpstack_api::internal;
use lrwn::EUI64;

pub async fn save_rx_info(rx_info: &internal::DeviceGatewayRxInfo) -> Result<()> {
    let dev_eui = EUI64::from_slice(&rx_info.dev_eui)?;
    let conf = config::get();
    let key = redis_key(format!("device:{{{}}}:gwrx", dev_eui));
    let ttl = conf.network.device_session_ttl.as_millis() as usize;
    let b = rx_info.encode_to_vec();

    () = redis::cmd("PSETEX")
        .arg(key)
        .arg(ttl)
        .arg(b)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(dev_eui = %dev_eui, "Gateway rx-info saved");
    Ok(())
}

pub async fn get_rx_info(dev_eui: &EUI64) -> Result<internal::DeviceGatewayRxInfo, Error> {
    let key = redis_key(format!("device:{{{}}}:gwrx", dev_eui));

    let b: Vec<u8> = redis::cmd("GET")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await
        .context("Get rx-info")?;
    if b.is_empty() {
        return Err(Error::NotFound(dev_eui.to_string()));
    }

    Ok(internal::DeviceGatewayRxInfo::decode(&mut Cursor::new(b)).context("Decode rx-info")?)
}

pub async fn get_rx_info_for_dev_euis(
    dev_euis: &[EUI64],
) -> Result<Vec<internal::DeviceGatewayRxInfo>, Error> {
    if dev_euis.is_empty() {
        return Ok(Vec::new());
    }

    let mut keys: Vec<String> = Vec::new();
    for dev_eui in dev_euis {
        keys.push(redis_key(format!("device:{{{}}}:gwrx", dev_eui)));
    }

    let bb: Vec<Vec<u8>> = redis::cmd("MGET")
        .arg(keys)
        .query_async(&mut get_async_redis_conn().await?)
        .await
        .context("MGET")?;
    let mut out: Vec<internal::DeviceGatewayRxInfo> = Vec::new();
    for b in bb {
        if b.is_empty() {
            continue;
        }

        out.push(
            internal::DeviceGatewayRxInfo::decode(&mut Cursor::new(b)).context("Decode rx-info")?,
        );
    }
    Ok(out)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::test;

    #[tokio::test]
    async fn test_rx_info() {
        let _guard = test::prepare().await;
        let rx_info = internal::DeviceGatewayRxInfo {
            dev_eui: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            ..Default::default()
        };
        let dev_eui = EUI64::from_slice(&rx_info.dev_eui).unwrap();

        // save
        save_rx_info(&rx_info).await.unwrap();

        // get
        let res = get_rx_info(&dev_eui).await.unwrap();
        assert_eq!(rx_info, res);
    }
}

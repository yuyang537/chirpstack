/*
 * 模块概述
 * ========
 * device_session模块提供了用于管理LoRaWAN设备会话数据的存储和检索功能。设备会话包含了
 * 设备的当前状态信息，如加密密钥、帧计数器、MAC命令状态等，这些信息对于维持设备与网络服务器
 * 之间的安全通信至关重要。
 * 
 * 该模块使用Redis作为存储后端，将设备会话数据序列化为Protocol Buffers格式存储。这种方式
 * 提供了高效的数据访问和更新能力，同时保持了数据结构的灵活性。设备会话数据通过设备的EUI
 * (扩展唯一标识符)进行索引，确保每个设备的会话数据能够被快速定位和检索。
 *
 * 文件功能
 * ========
 * 本文件(device_session.rs)实现了设备会话数据的存储和检索功能，提供了以下主要功能：
 * 1. 根据设备EUI获取设备会话数据
 * 2. 将设备会话数据保存到Redis存储中
 * 3. 处理设备会话数据的序列化和反序列化
 *
 * 主要组件
 * ========
 * - get函数: 根据设备EUI从Redis中检索设备会话数据
 * - save函数: 将设备会话数据保存到Redis中
 * - delete函数: 从Redis中删除设备会话数据
 * - Protocol Buffers序列化/反序列化: 用于高效地编码和解码设备会话数据
 *
 * 关键流程
 * ========
 * 1. 设备会话检索流程:
 *    - 根据设备EUI构造Redis键
 *    - 从Redis获取序列化的设备会话数据
 *    - 使用Protocol Buffers将二进制数据解码为DeviceSession结构
 *    - 返回设备会话数据或NotFound错误
 *
 * 2. 设备会话保存流程:
 *    - 使用Protocol Buffers将DeviceSession结构编码为二进制数据
 *    - 根据设备EUI构造Redis键
 *    - 将序列化数据保存到Redis中，设置适当的过期时间
 *
 * 注意事项
 * ========
 * - 数据一致性: 设备会话数据的一致性对于LoRaWAN通信至关重要
 * - 性能考虑: Redis提供高性能的数据访问，但需要注意内存使用
 * - 安全性: 设备会话包含敏感的安全密钥，需要保护Redis访问
 * - 过期策略: 设备会话数据设置了过期时间，以防止无限制地增长
 * - 错误处理: 适当处理Redis连接和数据序列化过程中的错误
 */

use std::io::Cursor;

use anyhow::{Context, Result};
use prost::Message;

use super::error::Error;
use super::{get_async_redis_conn, redis_key};
use chirpstack_api::internal;
use lrwn::EUI64;

pub async fn get(dev_eui: &EUI64) -> Result<internal::DeviceSession, Error> {
    let key = redis_key(format!("device:{{{}}}:ds", dev_eui));

    let v: Vec<u8> = redis::cmd("GET")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await
        .context("Get device-session")?;
    if v.is_empty() {
        return Err(Error::NotFound(dev_eui.to_string()));
    }
    let ds =
        internal::DeviceSession::decode(&mut Cursor::new(v)).context("Decode device-session")?;
    Ok(ds)
}

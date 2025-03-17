/*
 * 模块概述
 * ========
 * mac_command模块提供了用于管理LoRaWAN MAC命令的临时存储和检索功能。MAC命令是LoRaWAN
 * 协议中用于网络服务器和终端设备之间交换控制信息的机制，包括链路检查、数据速率调整、
 * 信道配置等操作。这些命令在设备通信过程中起着关键的网络管理作用。
 * 
 * 该模块使用Redis作为存储后端，将MAC命令集合序列化为二进制格式存储。这种方式提供了高效的
 * 数据访问能力，同时支持命令的临时保存和状态跟踪。MAC命令通过设备EUI和命令类型ID(CID)
 * 进行索引，确保每个设备的不同类型MAC命令能够被独立管理。
 *
 * 文件功能
 * ========
 * 本文件(mac_command.rs)实现了MAC命令的存储和检索功能，提供了以下主要功能：
 * 1. 设置待处理的MAC命令集合
 * 2. 获取待处理的MAC命令集合
 * 3. 删除待处理的MAC命令集合
 * 4. 处理MAC命令集合的序列化和反序列化
 *
 * 主要组件
 * ========
 * - set_pending函数: 将MAC命令集合保存为待处理状态
 * - get_pending函数: 获取待处理的MAC命令集合
 * - delete_pending函数: 删除待处理的MAC命令集合
 * - MAC命令序列化/反序列化: 用于高效地编码和解码MAC命令数据
 *
 * 关键流程
 * ========
 * 1. MAC命令设置流程:
 *    - 根据设备EUI和命令类型ID构造Redis键
 *    - 将MAC命令集合序列化为二进制数据
 *    - 将序列化数据保存到Redis中，设置与设备会话相同的过期时间
 *    - 记录日志，表明MAC命令已设置为待处理状态
 *
 * 2. MAC命令检索流程:
 *    - 根据设备EUI和命令类型ID构造Redis键
 *    - 从Redis获取序列化的MAC命令数据
 *    - 将二进制数据解析为MAC命令集合
 *    - 设置适当的上行标志并解码原始数据
 *    - 返回MAC命令集合或None（如果不存在）
 *
 * 注意事项
 * ========
 * - 命令状态管理: 模块跟踪MAC命令的待处理状态，确保命令的正确执行和确认
 * - 过期策略: MAC命令数据使用与设备会话相同的过期时间，防止资源泄漏
 * - 性能考虑: Redis提供高性能的数据访问，适合频繁的MAC命令操作
 * - 错误处理: 适当处理Redis连接和数据序列化过程中的错误
 * - 测试支持: 包含测试模块，用于验证MAC命令存储和检索功能
 */

use anyhow::Result;
use tracing::info;

use super::{get_async_redis_conn, redis_key};
use crate::config;
use lrwn::EUI64;

pub async fn set_pending(dev_eui: &EUI64, cid: lrwn::CID, set: &lrwn::MACCommandSet) -> Result<()> {
    let conf = config::get();

    let key = redis_key(format!("device:{}:mac:pending:{}", dev_eui, cid.to_u8()));
    let ttl = conf.network.device_session_ttl.as_millis() as usize;
    let b = set.to_vec()?;

    () = redis::cmd("PSETEX")
        .arg(key)
        .arg(ttl)
        .arg(b)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(dev_eui = %dev_eui, cid = %cid, "Pending mac-command block set");
    Ok(())
}

pub async fn get_pending(dev_eui: &EUI64, cid: lrwn::CID) -> Result<Option<lrwn::MACCommandSet>> {
    let key = redis_key(format!("device:{}:mac:pending:{}", dev_eui, cid.to_u8()));
    let b: Vec<u8> = redis::cmd("GET")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    let out = if !b.is_empty() {
        let mut mac = lrwn::MACCommandSet::from_slice(&b);

        // Per definition, the uplink flag is set to false as this function is intended to retrieve
        // pending mac-commands that were previously sent to the device.
        mac.decode_from_raw(false)?;

        Some(mac)
    } else {
        None
    };

    Ok(out)
}

pub async fn delete_pending(dev_eui: &EUI64, cid: lrwn::CID) -> Result<()> {
    let key = redis_key(format!("device:{}:mac:pending:{}", dev_eui, cid.to_u8()));

    () = redis::cmd("DEL")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(dev_eui = %dev_eui, cid = %cid, "Pending mac-command block deleted");
    Ok(())
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::test;

    #[tokio::test]
    async fn test_mac_command() {
        let _guard = test::prepare().await;

        let dev_eui = EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]);
        let mac = lrwn::MACCommandSet::new(vec![lrwn::MACCommand::DevStatusReq]);

        // set
        set_pending(&dev_eui, lrwn::CID::DevStatusReq, &mac)
            .await
            .unwrap();

        // get
        let mac_get = get_pending(&dev_eui, lrwn::CID::DevStatusReq)
            .await
            .unwrap();
        assert_eq!(mac, mac_get.unwrap());

        // delete
        delete_pending(&dev_eui, lrwn::CID::DevStatusReq)
            .await
            .unwrap();
        let resp = get_pending(&dev_eui, lrwn::CID::DevStatusReq)
            .await
            .unwrap();
        assert!(resp.is_none());
    }
}

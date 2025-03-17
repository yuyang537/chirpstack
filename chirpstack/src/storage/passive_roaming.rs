/*
 * 模块概述
 * ========
 * passive_roaming模块提供了用于管理LoRaWAN被动漫游设备会话的存储和检索功能。被动漫游
 * 是LoRaWAN 1.1规范中引入的功能，允许设备在访问网络（非本地网络）中发送上行消息，而这些
 * 消息会被转发到设备的本地网络进行处理。ChirpStack在此模块中实现了作为访问网络的功能。
 * 
 * 该模块使用Redis作为存储后端，将被动漫游设备会话序列化为Protocol Buffers格式存储。
 * 这种方式提供了高效的数据访问能力，同时支持会话的临时性质和生命周期管理。被动漫游会话
 * 通过会话ID(UUID)进行唯一标识，并通过设备地址(DevAddr)和设备EUI(DevEUI)进行索引。
 *
 * 文件功能
 * ========
 * 本文件(passive_roaming.rs)实现了被动漫游设备会话的存储和管理功能，提供了以下主要功能：
 * 1. 保存被动漫游设备会话
 * 2. 根据会话ID获取和删除被动漫游设备会话
 * 3. 根据物理载荷(PHYPayload)查找匹配的被动漫游设备会话
 * 4. 根据设备地址和设备EUI查找相关的会话ID
 * 5. 处理帧计数器(FCnt)的完整值计算
 *
 * 主要组件
 * ========
 * - save函数: 保存被动漫游设备会话到Redis
 * - get函数: 根据会话ID获取被动漫游设备会话
 * - delete函数: 根据会话ID删除被动漫游设备会话
 * - get_for_phy_payload函数: 根据物理载荷查找匹配的被动漫游设备会话
 * - get_sessions_for_dev_addr函数: 获取与设备地址关联的所有会话
 * - get_session_ids_for_dev_addr函数: 获取与设备地址关联的所有会话ID
 * - get_session_ids_for_dev_eui函数: 获取与设备EUI关联的所有会话ID
 * - get_full_f_cnt_up函数: 计算上行帧计数器的完整值
 *
 * 关键流程
 * ========
 * 1. 被动漫游会话保存流程:
 *    - 验证会话的生命周期是否有效
 *    - 将会话序列化为二进制数据
 *    - 在Redis中创建会话ID、设备地址和设备EUI之间的关联
 *    - 设置适当的过期时间，确保会话自动清理
 *
 * 2. 上行消息处理流程:
 *    - 从上行物理载荷中提取设备地址
 *    - 查找与该设备地址关联的所有被动漫游会话
 *    - 验证MIC以确定正确的会话
 *    - 处理帧计数器并更新会话状态
 *
 * 3. 会话清理流程:
 *    - 根据会话ID删除会话数据
 *    - 移除设备地址和设备EUI到会话ID的关联
 *    - 记录会话删除的日志
 *
 * 注意事项
 * ========
 * - 会话生命周期: 被动漫游会话具有有限的生命周期，由本地网络指定
 * - 索引策略: 通过设备地址和设备EUI双重索引，提高查询效率
 * - 帧计数器处理: 需要特殊处理16位截断的帧计数器，恢复完整的32位值
 * - 安全性: 会话包含敏感的安全密钥，需要保护Redis访问
 * - 性能考虑: Redis提供高性能的数据访问，适合频繁的会话查询
 * - 错误处理: 适当处理Redis连接和数据序列化过程中的错误
 */

use std::io::Cursor;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use prost::Message;
use tracing::{debug, info};
use uuid::Uuid;

use super::error::Error;
use super::{get_async_redis_conn, redis_key};
use crate::config;
use chirpstack_api::internal;
use lrwn::{AES128Key, DevAddr, EUI64};

pub async fn save(ds: &internal::PassiveRoamingDeviceSession) -> Result<()> {
    let sess_id = Uuid::from_slice(&ds.session_id)?;
    let dev_addr = DevAddr::from_slice(&ds.dev_addr)?;
    let dev_eui = if ds.dev_eui.is_empty() {
        EUI64::default()
    } else {
        EUI64::from_slice(&ds.dev_eui)?
    };

    let lifetime: DateTime<Utc> = match ds.lifetime {
        Some(v) => v.try_into().map_err(anyhow::Error::msg)?,
        None => {
            debug!("Not saving passive-roaming device-session, no passive-roaming lifetime set");
            return Ok(());
        }
    };

    let lifetime = lifetime - Utc::now();
    if lifetime <= Duration::zero() {
        debug!("Not saving passive-roaming device-session, lifetime of passive-roaming session expired");
        return Ok(());
    }

    let conf = config::get();

    let dev_addr_key = redis_key(format!("pr:devaddr:{{{}}}", dev_addr));
    let dev_eui_key = redis_key(format!("pr:dev:{{{}}}", dev_eui));
    let sess_key = redis_key(format!("pr:sess:{{{}}}", sess_id));
    let b = ds.encode_to_vec();
    let ttl = conf.network.device_session_ttl.as_millis() as usize;
    let pr_ttl = lifetime.num_milliseconds() as usize;

    // We need to store a pointer from both the DevAddr and DevEUI to the
    // passive-roaming device-session ID. This is needed:
    //  * Because the DevAddr is not guaranteed to be unique
    //  * Because the DevEUI might not be given (thus is also not guaranteed
    //    to be an unique identifier).
    //
    // But:
    //  * We need to be able to lookup the session using the DevAddr (potentially
    //    using the MIC validation).
    //  * We need to be able to stop a passive-roaming session given a DevEUI.
    () = redis::pipe()
        .atomic()
        .cmd("SADD")
        .arg(&dev_addr_key)
        .arg(sess_id.to_string())
        .ignore()
        .cmd("SADD")
        .arg(&dev_eui_key)
        .arg(sess_id.to_string())
        .ignore()
        .cmd("PEXPIRE")
        .arg(&dev_addr_key)
        .arg(ttl)
        .ignore()
        .cmd("PEXPIRE")
        .arg(&dev_eui_key)
        .arg(ttl)
        .ignore()
        .cmd("PSETEX")
        .arg(&sess_key)
        .arg(pr_ttl)
        .arg(b)
        .ignore()
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(id = %sess_id, "Passive-roaming device-session saved");

    Ok(())
}

pub async fn get(id: Uuid) -> Result<internal::PassiveRoamingDeviceSession, Error> {
    let key = redis_key(format!("pr:sess:{{{}}}", id));

    let v: Vec<u8> = redis::cmd("GET")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await
        .context("Get passive-roaming device-session")?;
    if v.is_empty() {
        return Err(Error::NotFound(id.to_string()));
    }
    let ds = internal::PassiveRoamingDeviceSession::decode(&mut Cursor::new(v))
        .context("Decode passive-roaming device-session")?;
    Ok(ds)
}

pub async fn delete(id: Uuid) -> Result<()> {
    let key = redis_key(format!("pr:sess:{{{}}}", id));

    () = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    info!(id = %id, "Passive-roaming device-session deleted");
    Ok(())
}

pub async fn get_for_phy_payload(
    phy: &lrwn::PhyPayload,
) -> Result<Vec<internal::PassiveRoamingDeviceSession>, Error> {
    // Clone the PhyPayload, as we will update the f_cnt to the full (32bit) frame-counter value
    // for calculating the MIC.
    let mut phy = phy.clone();

    let (dev_addr, f_cnt_orig) = if let lrwn::Payload::MACPayload(v) = &phy.payload {
        (v.fhdr.devaddr, v.fhdr.f_cnt)
    } else {
        return Err(Error::InvalidPayload("MacPayload".to_string()));
    };

    let sessions = get_sessions_for_dev_addr(dev_addr).await?;
    let mut out: Vec<internal::PassiveRoamingDeviceSession> = Vec::new();

    for ds in sessions {
        // We will not validate the MIC.
        if !ds.validate_mic {
            out.push(ds);
            continue;
        }

        let f_nwk_s_int_key = AES128Key::from_slice(&ds.f_nwk_s_int_key)?;

        // Set the full frame-counter.
        if let lrwn::Payload::MACPayload(pl) = &mut phy.payload {
            pl.fhdr.f_cnt = get_full_f_cnt_up(ds.f_cnt_up, f_cnt_orig);
        }

        let mic_ok = if ds.lorawan_1_1 {
            phy.validate_uplink_data_micf(&f_nwk_s_int_key)?
        } else {
            phy.validate_uplink_data_mic(
                lrwn::MACVersion::LoRaWAN1_0,
                0,
                0,
                0,
                &f_nwk_s_int_key,
                &f_nwk_s_int_key,
            )?
        };

        if mic_ok {
            out.push(ds);
        }
    }

    Ok(out)
}

async fn get_sessions_for_dev_addr(
    dev_addr: DevAddr,
) -> Result<Vec<internal::PassiveRoamingDeviceSession>> {
    let mut out: Vec<internal::PassiveRoamingDeviceSession> = Vec::new();
    let ids = get_session_ids_for_dev_addr(dev_addr).await?;

    for id in ids {
        if let Ok(v) = get(id).await {
            out.push(v);
        }
    }

    Ok(out)
}

async fn get_session_ids_for_dev_addr(dev_addr: DevAddr) -> Result<Vec<Uuid>> {
    let key = redis_key(format!("pr:devaddr:{{{}}}", dev_addr));

    let v: Vec<String> = redis::cmd("SMEMBERS")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    let mut out: Vec<Uuid> = Vec::new();
    for id in &v {
        out.push(Uuid::from_str(id)?);
    }

    Ok(out)
}

pub async fn get_session_ids_for_dev_eui(dev_eui: EUI64) -> Result<Vec<Uuid>> {
    let key = redis_key(format!("pr:dev:{{{}}}", dev_eui));

    let v: Vec<String> = redis::cmd("SMEMBERS")
        .arg(key)
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    let mut out: Vec<Uuid> = Vec::new();
    for id in &v {
        out.push(Uuid::from_str(id)?);
    }

    Ok(out)
}

fn get_full_f_cnt_up(next_expected_full_fcnt: u32, truncated_f_cnt: u32) -> u32 {
    // Handle re-transmission.
    if truncated_f_cnt == (((next_expected_full_fcnt % (1 << 16)) as u16).wrapping_sub(1)) as u32 {
        return next_expected_full_fcnt - 1;
    }

    let gap = ((truncated_f_cnt as u16).wrapping_sub((next_expected_full_fcnt % (1 << 16)) as u16))
        as u32;

    next_expected_full_fcnt.wrapping_add(gap)
}

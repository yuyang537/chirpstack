/*
 * 模块概述
 * ========
 * 帧日志(Frame)模块是ChirpStack LoRaWAN网络服务器流处理框架的重要组件，负责记录和管理
 * LoRaWAN通信帧的详细日志。该模块捕获设备与网关之间的上行和下行通信帧，提供了完整的
 * 通信记录，对于网络监控、故障排除和性能分析至关重要。
 * 
 * 帧日志模块利用Redis流(Redis Streams)作为存储机制，为每个网关和设备维护独立的帧日志流，
 * 同时也提供全局帧日志视图。这种设计使得系统能够高效地存储和查询大量的通信帧数据，同时
 * 支持按设备或网关进行过滤和分析。
 * 
 * 该模块记录的帧日志包含完整的物理载荷(PHYPayload)、发送信息(TxInfo)和接收信息(RxInfo)，
 * 以及解密后的帧选项(FOpts)和帧载荷(FRMPayload)等关键数据，为网络运维和故障诊断提供了
 * 全面的信息支持。
 *
 * 文件功能
 * ========
 * 本文件(frame.rs)实现了LoRaWAN帧日志的记录和查询功能，提供了以下主要功能：
 * 1. 记录设备上行帧日志，按网关和设备分别存储
 * 2. 记录设备下行帧日志，按网关和设备分别存储
 * 3. 查询历史帧日志，支持按时间范围和数量限制
 * 4. 将帧日志转换为API可用的日志项格式
 *
 * 主要组件
 * ========
 * - log_uplink_for_gateways(): 记录网关接收的上行帧日志
 * - log_downlink_for_gateway(): 记录网关发送的下行帧日志
 * - log_uplink_for_device(): 记录设备发送的上行帧日志
 * - log_downlink_for_device(): 记录设备接收的下行帧日志
 * - get_frame_logs(): 查询指定键的帧日志历史记录
 * - handle_stream(): 处理Redis流中的帧日志条目
 *
 * 关键流程
 * ========
 * 1. 上行帧日志记录流程:
 *    - 接收上行帧日志对象(UplinkFrameLog)
 *    - 为每个接收网关创建单独的日志条目
 *    - 将日志条目添加到相应的网关和设备流中
 *    - 应用配置的历史记录限制和过期时间
 *
 * 2. 下行帧日志记录流程:
 *    - 接收下行帧日志对象(DownlinkFrameLog)
 *    - 将日志条目添加到相应的网关和设备流中
 *    - 应用配置的历史记录限制和过期时间
 *
 * 3. 帧日志查询流程:
 *    - 接收查询请求，包含目标键和数量限制
 *    - 从Redis流中读取指定数量的日志条目
 *    - 解析日志条目并转换为API日志项格式
 *    - 通过通道发送日志项给调用者
 *
 * 注意事项
 * ========
 * - 性能考虑: 帧日志可能产生大量数据，需要合理配置历史记录限制
 * - 存储优化: 使用MAXLEN参数限制Redis流的大小，防止内存过度使用
 * - 过期策略: 为设备日志设置TTL，确保旧数据自动清理
 * - 错误处理: 日志记录失败不应影响主要业务流程
 * - 并发处理: 使用异步处理确保日志记录不阻塞主线程
 * - 数据安全: 日志中可能包含敏感信息，需要适当保护
 */

use std::io::Cursor;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use prost::Message;
use redis::streams::StreamReadReply;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, trace, warn};

use lrwn::EUI64;

use crate::config;
use crate::helpers::errors::PrintFullError;
use crate::storage::{get_async_redis_conn, redis_key};
use chirpstack_api::{api, stream};

pub async fn log_uplink_for_gateways(ufl: &stream::UplinkFrameLog) -> Result<()> {
    let conf = config::get();

    for rx_info in &ufl.rx_info {
        let gateway_id = EUI64::from_str(&rx_info.gateway_id)?;

        let ufl_copy = stream::UplinkFrameLog {
            phy_payload: ufl.phy_payload.clone(),
            tx_info: ufl.tx_info.clone(),
            rx_info: vec![rx_info.clone()],
            m_type: ufl.m_type,
            dev_addr: ufl.dev_addr.clone(),
            dev_eui: ufl.dev_eui.clone(),
            time: ufl.time,
            plaintext_f_opts: ufl.plaintext_f_opts,
            plaintext_frm_payload: ufl.plaintext_frm_payload,
        };

        let b = ufl_copy.encode_to_vec();

        // per gateway stream
        if conf.monitoring.per_gateway_frame_log_max_history > 0 {
            let key = redis_key(format!("gw:{{{}}}:stream:frame", gateway_id));

            () = redis::pipe()
                .atomic()
                .cmd("XADD")
                .arg(&key)
                .arg("MAXLEN")
                .arg(conf.monitoring.per_gateway_frame_log_max_history)
                .arg("*")
                .arg("up")
                .arg(&b)
                .ignore()
                .cmd("PEXPIRE")
                .arg(&key)
                .arg(conf.monitoring.per_gateway_frame_log_ttl.as_millis() as usize)
                .ignore()
                .query_async(&mut get_async_redis_conn().await?)
                .await?;
        }

        // global gateway stream
        if conf.monitoring.gateway_frame_log_max_history > 0 {
            let key = redis_key("gw:stream:frame".to_string());
            () = redis::cmd("XADD")
                .arg(&key)
                .arg("MAXLEN")
                .arg(conf.monitoring.gateway_frame_log_max_history)
                .arg("*")
                .arg("up")
                .arg(&b)
                .query_async(&mut get_async_redis_conn().await?)
                .await?;
        }
    }

    Ok(())
}

pub async fn log_downlink_for_gateway(dfl: &stream::DownlinkFrameLog) -> Result<()> {
    if dfl.gateway_id.is_empty() {
        return Err(anyhow!("gateway_id must be set"));
    }

    let conf = config::get();

    let b = dfl.encode_to_vec();

    // per gateway stream
    if conf.monitoring.per_gateway_frame_log_max_history > 0 {
        let key = redis_key(format!("gw:{{{}}}:stream:frame", dfl.gateway_id));
        () = redis::pipe()
            .atomic()
            .cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.per_gateway_frame_log_max_history)
            .arg("*")
            .arg("down")
            .arg(&b)
            .ignore()
            .cmd("PEXPIRE")
            .arg(&key)
            .arg(conf.monitoring.per_gateway_frame_log_ttl.as_millis() as usize)
            .ignore()
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    // global gateway stream
    if conf.monitoring.gateway_frame_log_max_history > 0 {
        let key = redis_key("gw:stream:frame".to_string());
        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.gateway_frame_log_max_history)
            .arg("*")
            .arg("down")
            .arg(&b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

pub async fn log_uplink_for_device(ufl: &stream::UplinkFrameLog) -> Result<()> {
    if ufl.dev_eui.is_empty() {
        return Err(anyhow!("dev_eui must be set"));
    }

    let conf = config::get();

    let b = ufl.encode_to_vec();

    // per device stream
    if conf.monitoring.per_device_frame_log_max_history > 0 {
        let key = redis_key(format!("device:{{{}}}:stream:frame", ufl.dev_eui));

        () = redis::pipe()
            .atomic()
            .cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.per_device_frame_log_max_history)
            .arg("*")
            .arg("up")
            .arg(&b)
            .ignore()
            .cmd("PEXPIRE")
            .arg(&key)
            .arg(conf.monitoring.per_device_frame_log_ttl.as_millis() as usize)
            .ignore()
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    // global device stream
    if conf.monitoring.device_frame_log_max_history > 0 {
        let key = redis_key("device:stream:frame".to_string());
        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.device_frame_log_max_history)
            .arg("*")
            .arg("up")
            .arg(&b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

pub async fn log_downlink_for_device(dfl: &stream::DownlinkFrameLog) -> Result<()> {
    if dfl.dev_eui.is_empty() {
        return Err(anyhow!("dev_eui must be set"));
    }

    let conf = config::get();

    let b = dfl.encode_to_vec();

    // per device stream
    if conf.monitoring.per_device_frame_log_max_history > 0 {
        let key = redis_key(format!("device:{{{}}}:stream:frame", dfl.dev_eui));

        () = redis::pipe()
            .atomic()
            .cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.per_device_frame_log_max_history)
            .arg("*")
            .arg("down")
            .arg(&b)
            .ignore()
            .cmd("PEXPIRE")
            .arg(&key)
            .arg(conf.monitoring.per_device_frame_log_ttl.as_millis() as usize)
            .ignore()
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    // global device stream
    if conf.monitoring.device_frame_log_max_history > 0 {
        let key = redis_key("device:stream:frame".to_string());
        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.device_frame_log_max_history)
            .arg("*")
            .arg("down")
            .arg(&b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

pub async fn get_frame_logs(
    key: String,
    count: usize,
    channel: mpsc::Sender<api::LogItem>,
) -> Result<()> {
    let mut last_id = "0".to_string();

    loop {
        if channel.is_closed() {
            debug!("Channel has been closed, returning");
            return Ok(());
        }

        let srr: StreamReadReply = redis::cmd("XREAD")
            .arg("COUNT")
            .arg(count)
            .arg("STREAMS")
            .arg(&key)
            .arg(&last_id)
            .query_async(&mut get_async_redis_conn().await?)
            .await
            .context("XREAD frame stream")?;

        for stream_key in &srr.keys {
            for stream_id in &stream_key.ids {
                last_id.clone_from(&stream_id.id);
                for (k, v) in &stream_id.map {
                    let res = handle_stream(&last_id, &channel, k, v).await;

                    if let Err(e) = res {
                        // Return in case of channel error, in any other case we just log
                        // the error.
                        if e.downcast_ref::<mpsc::error::SendError<api::LogItem>>()
                            .is_some()
                        {
                            return Err(e);
                        }

                        error!(key = %k, error = %e.full(), "Parsing frame-log error");
                    }
                }
            }
        }

        // If we use xread with block=0, the connection can't be used by other requests. Now we
        // check every 1 second if there are new messages, which should be sufficient.
        sleep(Duration::from_secs(1)).await;
    }
}

async fn handle_stream(
    stream_id: &str,
    channel: &mpsc::Sender<api::LogItem>,
    k: &str,
    v: &redis::Value,
) -> Result<()> {
    match k {
        "up" => {
            trace!(key = %k, id = %stream_id, "Frame-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = stream::UplinkFrameLog::decode(&mut Cursor::new(b))?;
                let mut phy = lrwn::PhyPayload::from_slice(&pl.phy_payload)?;
                if pl.plaintext_f_opts {
                    if let Err(e) = phy.decode_f_opts_to_mac_commands() {
                        warn!(error = %e.full(), "Decode f_opts to mac-commands error");
                    }
                }
                if pl.plaintext_frm_payload {
                    if let Err(e) = phy.decode_frm_payload() {
                        warn!(error = %e.full(), "Decode frm_payload error");
                    }
                }

                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|t| prost_types::Timestamp {
                        seconds: t.seconds,
                        nanos: t.nanos,
                    }),
                    description: pl.m_type().into(),
                    body: json!({
                        "phy_payload": phy,
                        "tx_info": pl.tx_info,
                        "rx_info": pl.rx_info,
                    })
                    .to_string(),
                    properties: [
                        ("DevAddr".to_string(), pl.dev_addr),
                        ("DevEUI".to_string(), pl.dev_eui),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        "down" => {
            trace!(key = %k, id = %stream_id, "frame-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = stream::DownlinkFrameLog::decode(&mut Cursor::new(b))?;
                let mut phy = lrwn::PhyPayload::from_slice(&pl.phy_payload)?;
                if pl.plaintext_f_opts {
                    if let Err(e) = phy.decode_f_opts_to_mac_commands() {
                        warn!(error = %e.full(), "Decode f_opts to mac-commands error");
                    }
                }
                if pl.plaintext_frm_payload {
                    if let Err(e) = phy.decode_frm_payload() {
                        warn!(error = %e.full(), "Decode frm_payload error");
                    }
                }

                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|t| prost_types::Timestamp {
                        seconds: t.seconds,
                        nanos: t.nanos,
                    }),
                    description: pl.m_type().into(),
                    body: json!({
                        "phy_payload": phy,
                        "tx_info": pl.tx_info,
                    })
                    .to_string(),
                    properties: [
                        ("DevAddr".to_string(), pl.dev_addr),
                        ("DevEUI".to_string(), pl.dev_eui),
                        ("Gateway ID".to_string(), pl.gateway_id),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        _ => {
            error!(key = %k, "Unexpected key in frame-log stream");
        }
    }

    Ok(())
}

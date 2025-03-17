/*
 * 模块概述
 * ========
 * 事件(Event)模块是ChirpStack LoRaWAN网络服务器流处理框架的核心组件，负责处理和分发
 * 系统中的各类事件。该模块捕获设备和应用程序生成的事件，如上行数据接收、下行数据发送、
 * 设备加入、设备状态变更等，并将这些事件记录到Redis流中，同时也支持将事件转发到外部
 * 集成系统。
 * 
 * 事件模块是ChirpStack与外部系统集成的关键桥梁，它提供了一种标准化的方式来监控和响应
 * LoRaWAN网络中的活动。通过事件流，应用程序可以实时获取设备数据，管理员可以监控网络
 * 状态，外部系统可以与ChirpStack无缝集成。
 * 
 * 该模块利用Redis流(Redis Streams)作为存储机制，为每个设备维护独立的事件流，同时也
 * 提供全局事件视图。这种设计使得系统能够高效地存储和查询大量的事件数据，同时支持按
 * 设备进行过滤和分析。
 *
 * 文件功能
 * ========
 * 本文件(event.rs)实现了事件的记录和查询功能，提供了以下主要功能：
 * 1. 记录设备事件，按设备分别存储
 * 2. 查询历史事件日志，支持按时间范围和数量限制
 * 3. 将事件日志转换为API可用的日志项格式
 * 4. 处理不同类型的事件，如上行、下行、加入和状态事件
 *
 * 主要组件
 * ========
 * - log_event_for_device(): 记录设备事件的主要函数
 * - get_event_logs(): 查询指定键的事件日志历史记录
 * - handle_stream(): 处理Redis流中的事件条目
 *
 * 关键流程
 * ========
 * 1. 设备事件记录流程:
 *    - 接收事件类型、设备EUI和事件数据
 *    - 将事件数据添加到设备特定的Redis流中
 *    - 将事件数据添加到全局设备事件流中
 *    - 应用配置的历史记录限制和过期时间
 *
 * 2. 事件日志查询流程:
 *    - 接收查询请求，包含目标键和数量限制
 *    - 从Redis流中读取指定数量的事件条目
 *    - 解析事件条目并转换为API日志项格式
 *    - 通过通道发送日志项给调用者
 *
 * 3. 事件处理流程:
 *    - 根据事件类型(上行、下行、加入、状态等)解析事件数据
 *    - 提取事件的关键信息，如时间戳、设备信息、数据内容等
 *    - 构建API日志项，包含事件类型、时间和格式化的事件数据
 *    - 通过通道发送日志项给调用者
 *
 * 注意事项
 * ========
 * - 性能考虑: 事件记录可能产生大量数据，需要合理配置历史记录限制
 * - 存储优化: 使用MAXLEN参数限制Redis流的大小，防止内存过度使用
 * - 过期策略: 为设备事件设置TTL，确保旧数据自动清理
 * - 错误处理: 事件记录失败不应影响主要业务流程
 * - 并发处理: 使用异步处理确保事件记录不阻塞主线程
 * - 数据安全: 事件中可能包含敏感信息，需要适当保护
 * - 集成扩展: 事件模块设计应支持未来添加新的事件类型和集成方式
 */

use std::io::Cursor;
use std::time::Duration;

use anyhow::{Context, Result};
use prost::Message;
use redis::streams::StreamReadReply;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, trace};

use crate::config;
use crate::helpers::errors::PrintFullError;
use crate::storage::{get_async_redis_conn, redis_key};
use chirpstack_api::{api, integration};

#[allow(clippy::enum_variant_names)]

pub async fn log_event_for_device(typ: &str, dev_eui: &str, b: &[u8]) -> Result<()> {
    let conf = config::get();

    // per device stream
    if conf.monitoring.per_device_event_log_max_history > 0 {
        let key = redis_key(format!("device:{{{}}}:stream:event", dev_eui));
        () = redis::pipe()
            .atomic()
            .cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.per_device_event_log_max_history)
            .arg("*")
            .arg(typ)
            .arg(b)
            .ignore()
            .cmd("PEXPIRE")
            .arg(&key)
            .arg(conf.monitoring.per_device_event_log_ttl.as_millis() as usize)
            .ignore()
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    // global device stream
    if conf.monitoring.device_event_log_max_history > 0 {
        let key = redis_key("device:stream:event".to_string());
        () = redis::cmd("XADD")
            .arg(&key)
            .arg("MAXLEN")
            .arg(conf.monitoring.device_event_log_max_history)
            .arg("*")
            .arg(typ)
            .arg(b)
            .query_async(&mut get_async_redis_conn().await?)
            .await?;
    }

    Ok(())
}

pub async fn get_event_logs(
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
            .context("XREAD event stream")?;

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
            trace!(key = %k, id = %stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::UplinkEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [
                        ("DR".to_string(), pl.dr.to_string()),
                        ("FPort".to_string(), pl.f_port.to_string()),
                        ("FCnt".to_string(), pl.f_cnt.to_string()),
                        ("Data".to_string(), hex::encode(&pl.data)),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        "join" => {
            trace!(key = %k, id = %stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::JoinEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [("DevAddr".to_string(), pl.dev_addr)]
                        .iter()
                        .cloned()
                        .collect(),
                };

                channel.send(pl).await?;
            }
        }
        "ack" => {
            trace!(key = %k, id = %stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::AckEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [].iter().cloned().collect(),
                };

                channel.send(pl).await?;
            }
        }
        "txack" => {
            trace!(key = %k, id = %stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::TxAckEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [].iter().cloned().collect(),
                };

                channel.send(pl).await?;
            }
        }
        "status" => {
            trace!(key = %k, id = %stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::StatusEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [
                        ("Margin".into(), format!("{}", pl.margin)),
                        ("Battery level".into(), format!("{:.0}%", pl.battery_level)),
                        (
                            "Battery level unavailable".into(),
                            format!("{}", pl.battery_level_unavailable),
                        ),
                        (
                            "External power source".into(),
                            format!("{}", pl.external_power_source),
                        ),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        "log" => {
            trace!(key = %k, id =%stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::LogEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [
                        ("Level".into(), pl.level().into()),
                        ("Code".into(), pl.code().into()),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        "location" => {
            trace!(key = %k, id=%stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::LocationEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [].iter().cloned().collect(),
                };

                channel.send(pl).await?;
            }
        }
        "integration" => {
            trace!(key = %k, id=%stream_id, "Event-log received from stream");
            if let redis::Value::BulkString(b) = v {
                let pl = integration::IntegrationEvent::decode(&mut Cursor::new(b))?;
                let pl = api::LogItem {
                    id: stream_id.to_string(),
                    time: pl.time.as_ref().map(|v| prost_types::Timestamp {
                        seconds: v.seconds,
                        nanos: v.nanos,
                    }),
                    description: k.to_string(),
                    body: serde_json::to_string(&pl)?,
                    properties: [
                        ("Integration".into(), pl.integration_name.clone()),
                        ("Event".into(), pl.event_type.clone()),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                };

                channel.send(pl).await?;
            }
        }
        _ => {
            error!(key = %k, "Unexpected key in in event-log stream");
        }
    }

    Ok(())
}

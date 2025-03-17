/**
 * @module uplink/mesh
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统中与LoRaWAN网状网络(Mesh Network)相关的心跳处理功能。它专注于处理
 * 来自网状网络中继设备的心跳消息，这些消息包含了中继设备的状态信息和网络拓扑数据。网状网络是LoRaWAN
 * 协议的扩展功能，允许设备通过中继节点扩展网络覆盖范围，特别适用于覆盖困难的区域。
 * 
 * # 文件功能
 * - 处理来自中继设备的网状网络心跳消息
 * - 更新中继设备的状态信息
 * - 创建或更新中继网关记录
 * - 维护网状网络的拓扑信息
 * - 监控中继设备的健康状态
 * 
 * # 主要组件
 * - MeshHeartbeat结构体：处理网状网络心跳消息的主要结构，包含处理过程中的所有状态和数据
 * - handle方法：处理心跳消息的入口点
 * - update_or_create_relay_gateway方法：更新或创建中继网关记录
 * 
 * # 关键流程
 * - 网状网络心跳处理流程：
 *   1. 接收中继设备的心跳消息
 *   2. 解析网关ID和中继ID
 *   3. 创建处理跨度(span)用于日志记录
 *   4. 更新或创建中继网关记录
 *   5. 记录中继设备的状态信息
 * 
 * # 重要考虑事项
 * - 网状网络功能需要特定的硬件支持和配置
 * - 中继设备的状态对整个网络的性能和可靠性至关重要
 * - 需要处理中继设备可能的离线或故障情况
 * - 网状网络拓扑可能会动态变化，需要及时更新
 * - 心跳消息的处理需要高效，以支持大量中继设备
 * - 网状网络功能是LoRaWAN协议的扩展，可能需要特定的协议版本支持
 */

use std::str::FromStr;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{error, span, trace, warn, Instrument, Level};

use chirpstack_api::gw;

use crate::config;
use crate::helpers::errors::PrintFullError;
use crate::storage::{
    error::Error,
    gateway::{self, RelayId},
};
use lrwn::EUI64;

pub struct MeshHeartbeat {
    gateway_id: EUI64,
    relay_id: RelayId,
    mesh_stats: gw::MeshHeartbeat,
}

impl MeshHeartbeat {
    pub async fn handle(s: gw::MeshHeartbeat) {
        let gateway_id = match EUI64::from_str(&s.gateway_id) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e.full(), "Decode gateway_id error");
                return;
            }
        };

        let relay_id = match RelayId::from_str(&s.relay_id) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e.full(), "Decode relay_id error");
                return;
            }
        };

        let span = span!(Level::INFO, "mesh_stats", gateway_id = %gateway_id, relay_id = %relay_id);

        if let Err(e) = MeshHeartbeat::_handle(gateway_id, relay_id, s)
            .instrument(span)
            .await
        {
            match e.downcast_ref::<Error>() {
                Some(Error::NotFound(_)) => {
                    let conf = config::get();
                    if !conf.gateway.allow_unknown_gateways {
                        error!(error = %e.full(), "Handle mesh-stats error");
                    }
                }
                Some(_) | None => {
                    error!(error = %e.full(), "Handle mesh-stats error");
                }
            }
        }
    }

    async fn _handle(gateway_id: EUI64, relay_id: RelayId, s: gw::MeshHeartbeat) -> Result<()> {
        let mut ctx = MeshHeartbeat {
            gateway_id,
            relay_id,
            mesh_stats: s,
        };

        ctx.update_or_create_relay_gateway().await?;

        Ok(())
    }

    async fn update_or_create_relay_gateway(&mut self) -> Result<()> {
        trace!("Getting Border Gateway");
        let border_gw = gateway::get(&self.gateway_id).await?;

        let ts: DateTime<Utc> = match &self.mesh_stats.time {
            Some(v) => (*v)
                .try_into()
                .map_err(|e| anyhow!("Convert time error: {}", e))?,
            None => {
                warn!("Stats message does not have time field set");
                return Ok(());
            }
        };

        match gateway::get_relay_gateway(border_gw.tenant_id.into(), self.relay_id).await {
            Ok(mut v) => {
                if let Some(last_seen_at) = v.last_seen_at {
                    if last_seen_at > ts {
                        warn!("Time is less than last seen timestamp, ignoring stats");
                        return Ok(());
                    }
                }

                v.last_seen_at = Some(ts);
                v.region_config_id = border_gw
                    .properties
                    .get("region_config_id")
                    .cloned()
                    .unwrap_or_default();
                gateway::update_relay_gateway(v).await?;
            }
            Err(_) => {
                let _ = gateway::create_relay_gateway(gateway::RelayGateway {
                    tenant_id: border_gw.tenant_id,
                    relay_id: self.relay_id,
                    name: self.relay_id.to_string(),
                    last_seen_at: Some(ts),
                    ..Default::default()
                })
                .await?;
            }
        }

        Ok(())
    }
}

/**
 * @module gateway/backend
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统与LoRaWAN网关之间的通信后端。它提供了一个抽象接口，
 * 允许系统通过不同的通信协议（如MQTT）与网关进行交互，同时管理不同区域的网关后端配置。
 * 
 * # 文件功能
 * - 定义网关后端的抽象接口
 * - 管理不同区域的网关后端实例
 * - 提供向网关发送下行数据和配置的功能
 * - 初始化和设置网关后端服务
 * 
 * # 主要组件
 * - GatewayBackend trait：定义网关后端的通用接口
 * - BACKENDS全局变量：存储不同区域的网关后端实例
 * - mqtt模块：实现基于MQTT协议的网关通信
 * - mock模块：提供用于测试的模拟网关后端
 * 
 * # 关键流程
 * - 系统启动时为每个启用的区域初始化网关后端
 * - 根据区域ID选择适当的网关后端进行通信
 * - 向网关发送下行数据帧
 * - 向网关发送配置信息
 * 
 * # 重要考虑事项
 * - 网关后端需要处理不同区域的频率计划和参数
 * - 通信协议的可靠性和安全性
 * - 支持多种网关类型和厂商
 * - 错误处理和重试机制
 */

use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::info;

use crate::config;

#[cfg(test)]
pub mod mock;
mod mqtt;

lazy_static! {
    static ref BACKENDS: RwLock<HashMap<String, Box<dyn GatewayBackend + Sync + Send>>> =
        RwLock::new(HashMap::new());
}

#[async_trait]
pub trait GatewayBackend {
    async fn send_downlink(&self, df: &chirpstack_api::gw::DownlinkFrame) -> Result<()>;
    async fn send_configuration(
        &self,
        gw_conf: &chirpstack_api::gw::GatewayConfiguration,
    ) -> Result<()>;
}

pub async fn setup() -> Result<()> {
    let conf = config::get();

    info!("Setting up gateway backends for the different regions");
    for region in &conf.regions {
        if !conf.network.enabled_regions.contains(&region.id) {
            continue;
        }

        info!(
            region_id = %region.id,
            region_common_name = %region.common_name,
            "Setting up gateway backend for region"
        );

        let backend =
            mqtt::MqttBackend::new(&region.id, region.common_name, &region.gateway.backend.mqtt)
                .await
                .context("New MQTT gateway backend error")?;

        set_backend(&region.id, Box::new(backend)).await;
    }

    Ok(())
}

pub async fn set_backend(region_config_id: &str, b: Box<dyn GatewayBackend + Sync + Send>) {
    let mut b_w = BACKENDS.write().await;
    b_w.insert(region_config_id.to_string(), b);
}

pub async fn send_downlink(
    region_config_id: &str,
    df: &chirpstack_api::gw::DownlinkFrame,
) -> Result<()> {
    let b_r = BACKENDS.read().await;
    let b = b_r.get(region_config_id).ok_or_else(|| {
        anyhow!(
            "region_config_id '{}' does not exist in BACKENDS",
            region_config_id
        )
    })?;

    b.send_downlink(df).await?;

    Ok(())
}

pub async fn send_configuration(
    region_config_id: &str,
    gw_conf: &chirpstack_api::gw::GatewayConfiguration,
) -> Result<()> {
    let b_r = BACKENDS.read().await;
    let b = b_r.get(region_config_id).ok_or_else(|| {
        anyhow!(
            "region_config_id '{}' does not exist in BACKENDS",
            region_config_id
        )
    })?;

    b.send_configuration(gw_conf).await?;

    Ok(())
}

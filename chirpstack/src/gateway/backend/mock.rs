/**
 * @module gateway/backend/mock
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了一个模拟的网关后端接口，主要用于测试和开发环境。它提供了一个符合GatewayBackend
 * trait的实现，但不实际与任何物理网关通信，而是将下行数据和配置信息存储在内存中，以便测试代码
 * 可以检查和验证这些数据。
 * 
 * # 文件功能
 * - 提供一个模拟的网关后端实现，用于测试和开发
 * - 存储发送到网关的下行数据帧和配置信息
 * - 提供接口以检索已发送的下行数据和配置
 * - 支持重置功能，清除所有存储的数据
 * 
 * # 主要组件
 * - Backend结构体：实现GatewayBackend trait的模拟后端
 * - DOWNLINK_FRAMES：存储发送到网关的下行数据帧
 * - GATEWAY_CONFIGURATIONS：存储发送到网关的配置信息
 * - reset()：清除所有存储的数据
 * - get_downlink_frames()：获取所有已发送的下行数据帧
 * - get_gateway_configurations()：获取所有已发送的网关配置
 * 
 * # 关键流程
 * - 当系统调用send_downlink()时，下行数据帧被存储而不是实际发送
 * - 当系统调用send_configuration()时，网关配置被存储而不是实际发送
 * - 测试代码可以通过get_downlink_frames()和get_gateway_configurations()检查这些数据
 * - 测试完成后可以通过reset()清除所有数据，准备下一次测试
 * 
 * # 重要考虑事项
 * - 此模块仅用于测试和开发，不应在生产环境中使用
 * - 模拟后端不会验证下行数据或配置的有效性
 * - 存储在内存中的数据在应用程序重启后会丢失
 * - 在并发测试中使用时需要注意数据隔离
 */

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

use chirpstack_api::gw;

use super::GatewayBackend;

lazy_static! {
    static ref DOWNLINK_FRAMES: RwLock<Vec<gw::DownlinkFrame>> = RwLock::new(Vec::new());
    static ref GATEWAY_CONFIGURATIONS: RwLock<Vec<gw::GatewayConfiguration>> =
        RwLock::new(Vec::new());
}

pub async fn reset() {
    DOWNLINK_FRAMES.write().await.drain(..);
    GATEWAY_CONFIGURATIONS.write().await.drain(..);
}

pub struct Backend {}

#[async_trait]
impl GatewayBackend for Backend {
    async fn send_downlink(&self, df: &chirpstack_api::gw::DownlinkFrame) -> Result<()> {
        DOWNLINK_FRAMES.write().await.push(df.clone());
        Ok(())
    }

    async fn send_configuration(
        &self,
        gw_conf: &chirpstack_api::gw::GatewayConfiguration,
    ) -> Result<()> {
        GATEWAY_CONFIGURATIONS.write().await.push(gw_conf.clone());
        Ok(())
    }
}

pub async fn get_downlink_frames() -> Vec<gw::DownlinkFrame> {
    DOWNLINK_FRAMES.write().await.drain(..).collect()
}

pub async fn get_gateway_configurations() -> Vec<gw::GatewayConfiguration> {
    GATEWAY_CONFIGURATIONS.write().await.drain(..).collect()
}

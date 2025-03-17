/**
 * @module uplink/data_sns
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统中与服务网络服务器(Serving Network Server, SNS)相关的上行数据处理功能。
 * 它专注于处理主动漫游(Active Roaming)场景下的LoRaWAN上行数据帧，作为服务网络服务器接收来自其他网络
 * 服务器的上行数据，并将其转发到相应的处理流程。这是LoRaWAN协议中支持设备跨网络漫游的数据处理组件。
 * 
 * # 文件功能
 * - 作为服务网络服务器处理来自其他网络的上行数据
 * - 将上行数据转发到标准的上行数据处理流程
 * - 提供与主动漫游相关的上行数据处理接口
 * 
 * # 主要组件
 * - Data结构体：处理服务网络上行数据的主要结构
 * - handle方法：处理上行数据的入口点，将数据转发到标准处理流程
 * 
 * # 关键流程
 * - 服务网络上行数据处理流程：
 *   1. 接收上行数据帧
 *   2. 创建跟踪跨度(span)用于日志记录
 *   3. 将数据转发到标准的上行数据处理流程(_handle方法)
 * 
 * # 重要考虑事项
 * - 作为服务网络服务器需要正确配置网络标识符(NetID)和安全凭证
 * - 上行数据的处理需要与标准处理流程保持一致
 * - 需要确保数据的安全性和完整性
 * - 需要遵循LoRaWAN后端接口规范中的漫游协议
 * - 与主动漫游相关的其他组件(如入网请求处理)需要协同工作
 */

use anyhow::Result;
use tracing::{span, Instrument, Level};

use super::{data, UplinkFrameSet};

pub struct Data {}

impl Data {
    pub async fn handle(ufs: UplinkFrameSet) -> Result<()> {
        let span = span!(Level::INFO, "data_up_sns", dev_eui = tracing::field::Empty);
        data::Data::_handle(ufs).instrument(span).await
    }
}

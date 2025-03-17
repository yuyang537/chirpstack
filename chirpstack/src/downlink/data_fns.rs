/*
 * 模块概述
 * ========
 * 漫游数据下行链路(Data FNS)模块是ChirpStack LoRaWAN网络服务器下行链路处理框架的专用组件，
 * 负责处理LoRaWAN漫游场景中的转发网络服务器(Forwarding Network Server, FNS)下行链路数据。
 * 该模块实现了LoRaWAN Backend Interfaces规范中定义的漫游数据下行链路处理机制，特别是
 * 处理来自主网络服务器(Home Network Server, HNS)的XmitDataReq请求。
 * 
 * 在LoRaWAN漫游架构中，当设备漫游到访问网络时，其上行数据由访问网络的转发网络服务器(FNS)
 * 接收并转发给主网络服务器(HNS)。当HNS需要向漫游设备发送下行数据时，它会发送XmitDataReq
 * 请求给FNS，由FNS负责将下行数据发送给设备。本模块就是处理这一过程的核心组件。
 * 
 * 本模块通过解析XmitDataReq请求，提取下行数据和元数据，选择合适的网关，设置下行链路参数，
 * 并将下行帧发送到网关，从而实现了完整的漫游下行链路通信流程。
 *
 * 文件功能
 * ========
 * 本文件(data_fns.rs)实现了漫游数据下行链路的处理逻辑，提供了以下主要功能：
 * 1. 处理来自HNS的XmitDataReq请求
 * 2. 解析下行元数据(DLMetaData)
 * 3. 基于上行接收信息选择最佳下行网关
 * 4. 设置下行链路参数（频率、数据速率、发送功率等）
 * 5. 构建下行链路帧
 * 6. 保存下行链路帧信息用于分析和故障排除
 * 7. 将下行链路帧发送到选定的网关
 *
 * 主要组件
 * ========
 * - Data: 核心结构体，包含漫游数据下行链路处理的所有状态和方法
 *   - handle(): 处理XmitDataReq请求的入口函数
 *   - set_downlink_frame(): 设置下行链路帧参数
 *   - save_downlink_frame(): 保存下行链路帧信息
 *   - send_downlink_frame(): 发送下行链路帧到网关
 *
 * 关键流程
 * ========
 * 1. 漫游数据下行链路处理流程:
 *    - 接收XmitDataReq请求和下行元数据
 *    - 将下行元数据转换为上行接收信息
 *    - 根据信号质量(SNR/RSSI)排序上行接收信息
 *    - 获取区域配置
 *    - 设置下行链路帧参数
 *    - 保存下行链路帧信息
 *    - 将下行链路帧发送到选定的网关
 *
 * 2. 下行链路帧设置流程:
 *    - 获取区域配置和网络配置
 *    - 设置下行链路帧头部信息
 *    - 设置下行链路帧项参数（频率、数据速率、发送功率等）
 *    - 设置下行链路帧网关信息
 *
 * 注意事项
 * ========
 * - 区域配置: 必须考虑不同区域的频率和参数要求
 * - 时间敏感性: 下行链路必须在特定时间窗口内发送
 * - 网关可用性: 选择的网关必须可用且能够发送下行链路
 * - 参数设置: 下行链路参数必须符合区域规范和设备能力
 * - 安全考虑: 漫游通信涉及多个网络，需要适当的安全措施
 * - 错误处理: 需要适当处理网关不可用或参数设置错误等情况
 * - 资源优化: 应优化网关选择以提高通信效率和可靠性
 */

use anyhow::{Context, Result};
use rand::Rng;
use tracing::{span, trace, Instrument, Level};

use super::helpers;
use crate::backend::roaming;
use crate::storage::downlink_frame;
use crate::{gateway, region};
use chirpstack_api::{gw, internal};

pub struct Data {
    region_config_id: String,
    xmit_data_req: backend::XmitDataReqPayload,
    dl_meta_data: backend::DLMetaData,
    uplink_rx_info: Vec<gw::UplinkRxInfo>,
    downlink_frame: gw::DownlinkFrame,
}

impl Data {
    pub async fn handle(
        pl: backend::XmitDataReqPayload,
        dl_meta: backend::DLMetaData,
    ) -> Result<()> {
        let span = span!(
            Level::INFO,
            "xmit_data_req_pr",
            transaction_id = pl.base.transaction_id
        );
        Data::_handle(pl, dl_meta).instrument(span).await
    }

    async fn _handle(pl: backend::XmitDataReqPayload, dl_meta: backend::DLMetaData) -> Result<()> {
        let mut uplink_rx_info = roaming::dl_meta_data_to_uplink_rx_info(&dl_meta)?;
        uplink_rx_info.sort_by(|a, b| {
            if a.snr == b.snr {
                return a.rssi.partial_cmp(&b.rssi).unwrap();
            }
            b.snr.partial_cmp(&a.snr).unwrap()
        });

        if uplink_rx_info.is_empty() {
            return Err(anyhow!("DLMetaData is not set"));
        }

        let region_config_id = uplink_rx_info[0]
            .metadata
            .get("region_config_id")
            .cloned()
            .unwrap_or_default();

        let mut ctx = Data {
            region_config_id,
            uplink_rx_info,
            xmit_data_req: pl,
            dl_meta_data: dl_meta,
            downlink_frame: gw::DownlinkFrame {
                downlink_id: rand::thread_rng().gen(),
                ..Default::default()
            },
        };

        ctx.set_downlink_frame()?;
        ctx.save_downlink_frame().await?;
        ctx.send_downlink_frame().await?;

        Ok(())
    }

    fn set_downlink_frame(&mut self) -> Result<()> {
        trace!("Setting DownlinkFrame parameters");
        let region_conf = region::get(&self.region_config_id)?;

        let rx_info = self
            .uplink_rx_info
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("rx_info is empty"))?;

        self.downlink_frame
            .gateway_id
            .clone_from(&rx_info.gateway_id);
        if self.dl_meta_data.dl_freq_1.is_some()
            && self.dl_meta_data.data_rate_1.is_some()
            && self.dl_meta_data.rx_delay_1.is_some()
        {
            let mut tx_info = gw::DownlinkTxInfo {
                frequency: (self.dl_meta_data.dl_freq_1.unwrap() * 1_000_000.0) as u32,
                board: rx_info.board,
                antenna: rx_info.antenna,
                context: rx_info.context.clone(),
                timing: Some(gw::Timing {
                    parameters: Some(gw::timing::Parameters::Delay(gw::DelayTimingInfo {
                        delay: Some(pbjson_types::Duration {
                            seconds: self.dl_meta_data.rx_delay_1.unwrap() as i64,
                            nanos: 0,
                        }),
                    })),
                }),
                ..Default::default()
            };

            tx_info.power = region_conf.get_downlink_tx_power_eirp(tx_info.frequency) as i32;

            let rx1_dr = region_conf.get_data_rate(self.dl_meta_data.data_rate_1.unwrap())?;
            helpers::set_tx_info_data_rate(&mut tx_info, &rx1_dr)?;

            self.downlink_frame.items.push(gw::DownlinkFrameItem {
                phy_payload: self.xmit_data_req.phy_payload.clone(),
                tx_info: Some(tx_info),
                tx_info_legacy: None,
            });
        }

        if self.dl_meta_data.dl_freq_2.is_some()
            && self.dl_meta_data.data_rate_2.is_some()
            && self.dl_meta_data.rx_delay_1.is_some()
        {
            let mut tx_info = gw::DownlinkTxInfo {
                frequency: (self.dl_meta_data.dl_freq_2.unwrap() * 1_000_000.0) as u32,
                board: rx_info.board,
                antenna: rx_info.antenna,
                context: rx_info.context,
                timing: Some(gw::Timing {
                    parameters: Some(gw::timing::Parameters::Delay(gw::DelayTimingInfo {
                        delay: Some(pbjson_types::Duration {
                            seconds: self.dl_meta_data.rx_delay_1.unwrap() as i64 + 1,
                            nanos: 0,
                        }),
                    })),
                }),
                ..Default::default()
            };

            tx_info.power = region_conf.get_downlink_tx_power_eirp(tx_info.frequency) as i32;

            let rx2_dr = region_conf.get_data_rate(self.dl_meta_data.data_rate_2.unwrap())?;
            helpers::set_tx_info_data_rate(&mut tx_info, &rx2_dr)?;

            self.downlink_frame.items.push(gw::DownlinkFrameItem {
                phy_payload: self.xmit_data_req.phy_payload.clone(),
                tx_info: Some(tx_info),
                tx_info_legacy: None,
            });
        }

        Ok(())
    }

    async fn save_downlink_frame(&self) -> Result<()> {
        trace!("Saving downlink frame");

        downlink_frame::save(&internal::DownlinkFrame {
            downlink_id: self.downlink_frame.downlink_id,
            downlink_frame: Some(self.downlink_frame.clone()),
            ..Default::default()
        })
        .await
        .context("Save downlink frame")?;

        Ok(())
    }

    async fn send_downlink_frame(&self) -> Result<()> {
        trace!("Sending downlink frame");

        gateway::backend::send_downlink(&self.region_config_id, &self.downlink_frame)
            .await
            .context("Send downlink frame")?;

        Ok(())
    }
}

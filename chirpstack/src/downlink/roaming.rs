/*
 * 模块概述
 * ========
 * 漫游下行链路(Roaming Downlink)模块是ChirpStack LoRaWAN网络服务器下行链路处理框架的专用组件，
 * 负责处理LoRaWAN漫游场景下的下行链路通信。在LoRaWAN漫游中，设备可能连接到非本地网络（访问网络），
 * 而其数据需要通过本地网络（服务网络）进行处理。
 * 
 * 该模块实现了LoRaWAN Backend Interfaces规范中定义的漫游下行链路处理机制，特别是被动漫游
 * (Passive Roaming)模式下的下行链路处理。在被动漫游中，访问网络负责接收设备上行数据并转发给
 * 服务网络，同时也负责将服务网络的下行数据发送给设备。
 * 
 * 本模块通过处理来自其他网络的下行链路请求，选择合适的网关，设置下行链路参数，并将下行帧发送
 * 到网关，从而实现了完整的漫游下行链路通信流程。
 *
 * 文件功能
 * ========
 * 本文件(roaming.rs)实现了漫游下行链路的处理逻辑，提供了以下主要功能：
 * 1. 处理被动漫游模式下的下行链路请求
 * 2. 基于上行链路信息选择最佳下行网关
 * 3. 设置下行链路参数（频率、数据速率、发送功率等）
 * 4. 构建下行链路帧
 * 5. 保存下行链路帧信息用于分析和故障排除
 * 6. 将下行链路帧发送到选定的网关
 *
 * 主要组件
 * ========
 * - PassiveRoamingDownlink: 核心结构体，包含漫游下行链路处理的所有状态和方法
 *   - handle(): 处理漫游下行链路请求的入口函数
 *   - select_downlink_gateway(): 选择最佳下行网关
 *   - set_downlink_frame(): 设置下行链路帧参数
 *   - save_downlink_frame(): 保存下行链路帧信息
 *   - send_downlink_frame(): 发送下行链路帧到网关
 *
 * 关键流程
 * ========
 * 1. 漫游下行链路处理流程:
 *    - 接收包含上行链路信息、物理载荷和下行元数据的请求
 *    - 基于上行链路信息选择最佳下行网关
 *    - 根据区域配置和下行元数据设置下行链路参数
 *    - 构建下行链路帧
 *    - 保存下行链路帧信息
 *    - 将下行链路帧发送到选定的网关
 *
 * 2. 网关选择流程:
 *    - 分析上行链路接收信息
 *    - 根据信号质量(RSSI/SNR)和其他因素选择最佳网关
 *    - 考虑网关状态和可用性
 *    - 返回选定的网关信息
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

use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use rand::Rng;
use tracing::{span, trace, Instrument, Level};

use super::helpers;
use crate::storage::downlink_frame;
use crate::uplink::UplinkFrameSet;
use crate::{config, gateway, region};
use backend::DLMetaData;
use chirpstack_api::{gw, internal};
use lrwn::EUI64;

pub struct PassiveRoamingDownlink {
    uplink_frame_set: UplinkFrameSet,
    phy_payload: Vec<u8>,
    dl_meta_data: DLMetaData,
    network_conf: config::RegionNetwork,
    region_conf: Arc<Box<dyn lrwn::region::Region + Sync + Send>>,
    downlink_frame: gw::DownlinkFrame,
    downlink_gateway: Option<internal::DeviceGatewayRxInfoItem>,
}

impl PassiveRoamingDownlink {
    pub async fn handle(ufs: UplinkFrameSet, phy: Vec<u8>, dl_meta: DLMetaData) -> Result<()> {
        let span = span!(Level::INFO, "passive_roaming");

        PassiveRoamingDownlink::_handle(ufs, phy, dl_meta)
            .instrument(span)
            .await
    }

    async fn _handle(ufs: UplinkFrameSet, phy: Vec<u8>, dl_meta: DLMetaData) -> Result<()> {
        let network_conf = config::get_region_network(&ufs.region_config_id)?;
        let region_conf = region::get(&ufs.region_config_id)?;

        let mut ctx = PassiveRoamingDownlink {
            uplink_frame_set: ufs,
            phy_payload: phy,
            dl_meta_data: dl_meta,
            network_conf,
            region_conf,
            downlink_frame: gw::DownlinkFrame {
                downlink_id: rand::thread_rng().gen(),
                ..Default::default()
            },
            downlink_gateway: None,
        };

        ctx.select_downlink_gateway()?;
        ctx.set_downlink_frame()?;
        ctx.save_downlink_frame().await?;
        ctx.send_downlink_frame().await?;

        Ok(())
    }

    fn select_downlink_gateway(&mut self) -> Result<()> {
        trace!("Selecting downlink gateway");

        let mut dev_gw_rx_info = internal::DeviceGatewayRxInfo {
            dev_eui: Vec::new(),
            dr: self.uplink_frame_set.dr as u32,
            items: self
                .uplink_frame_set
                .rx_info_set
                .iter()
                .map(|rx_info| {
                    let gw_id = EUI64::from_str(&rx_info.gateway_id).unwrap_or_default();

                    internal::DeviceGatewayRxInfoItem {
                        gateway_id: gw_id.to_vec(),
                        rssi: rx_info.rssi,
                        lora_snr: rx_info.snr,
                        antenna: rx_info.antenna,
                        board: rx_info.board,
                        context: rx_info.context.clone(),
                        is_private_up: self
                            .uplink_frame_set
                            .gateway_private_up_map
                            .get(&gw_id)
                            .cloned()
                            .unwrap_or_default(),
                        is_private_down: self
                            .uplink_frame_set
                            .gateway_private_down_map
                            .get(&gw_id)
                            .cloned()
                            .unwrap_or_default(),
                        tenant_id: self
                            .uplink_frame_set
                            .gateway_tenant_id_map
                            .get(&gw_id)
                            .map(|v| v.into_bytes().to_vec())
                            .unwrap_or_default(),
                    }
                })
                .collect(),
        };

        let gw_down = helpers::select_downlink_gateway(
            None,
            &self.uplink_frame_set.region_config_id,
            self.network_conf.gateway_prefer_min_margin,
            &mut dev_gw_rx_info,
        )?;

        self.downlink_frame.gateway_id = hex::encode(&gw_down.gateway_id);
        self.downlink_gateway = Some(gw_down);

        Ok(())
    }

    fn set_downlink_frame(&mut self) -> Result<()> {
        trace!("Setting downlink frame");

        let gw_down = self.downlink_gateway.as_ref().unwrap();

        if let Some(class_mode) = &self.dl_meta_data.class_mode {
            match class_mode.as_ref() {
                "A" => {
                    // RX1
                    if self.dl_meta_data.dl_freq_1.is_some()
                        && self.dl_meta_data.data_rate_1.is_some()
                        && self.dl_meta_data.rx_delay_1.is_some()
                    {
                        let dl_freq_1 = self.dl_meta_data.dl_freq_1.unwrap();
                        let dl_freq_1 = (dl_freq_1 * 1_000_000.0) as u32;
                        let data_rate_1 = self.dl_meta_data.data_rate_1.unwrap();
                        let data_rate_1 = self.region_conf.get_data_rate(data_rate_1)?;
                        let rx_delay_1 = self.dl_meta_data.rx_delay_1.unwrap();

                        let mut tx_info = gw::DownlinkTxInfo {
                            board: gw_down.board,
                            antenna: gw_down.antenna,
                            context: gw_down.context.clone(),
                            frequency: dl_freq_1,
                            timing: Some(gw::Timing {
                                parameters: Some(gw::timing::Parameters::Delay(
                                    gw::DelayTimingInfo {
                                        delay: Some(pbjson_types::Duration {
                                            seconds: rx_delay_1 as i64,
                                            nanos: 0,
                                        }),
                                    },
                                )),
                            }),
                            power: if self.network_conf.downlink_tx_power != -1 {
                                self.network_conf.downlink_tx_power
                            } else {
                                self.region_conf.get_downlink_tx_power_eirp(dl_freq_1) as i32
                            },
                            ..Default::default()
                        };
                        helpers::set_tx_info_data_rate(&mut tx_info, &data_rate_1)?;

                        self.downlink_frame.items.push(gw::DownlinkFrameItem {
                            phy_payload: self.phy_payload.clone(),
                            tx_info: Some(tx_info),
                            tx_info_legacy: None,
                        });
                    }

                    // RX2
                    if self.dl_meta_data.dl_freq_2.is_some()
                        && self.dl_meta_data.data_rate_2.is_some()
                        && self.dl_meta_data.rx_delay_1.is_some()
                    {
                        let dl_freq_2 = self.dl_meta_data.dl_freq_2.unwrap();
                        let dl_freq_2 = (dl_freq_2 * 1_000_000.0) as u32;
                        let data_rate_2 = self.dl_meta_data.data_rate_2.unwrap();
                        let data_rate_2 = self.region_conf.get_data_rate(data_rate_2)?;
                        let rx_delay_1 = self.dl_meta_data.rx_delay_1.unwrap();

                        let mut tx_info = gw::DownlinkTxInfo {
                            board: gw_down.board,
                            antenna: gw_down.antenna,
                            context: gw_down.context.clone(),
                            frequency: dl_freq_2,
                            timing: Some(gw::Timing {
                                parameters: Some(gw::timing::Parameters::Delay(
                                    gw::DelayTimingInfo {
                                        delay: Some(pbjson_types::Duration {
                                            seconds: (rx_delay_1 + 1) as i64,
                                            nanos: 0,
                                        }),
                                    },
                                )),
                            }),
                            power: if self.network_conf.downlink_tx_power != -1 {
                                self.network_conf.downlink_tx_power
                            } else {
                                self.region_conf.get_downlink_tx_power_eirp(dl_freq_2) as i32
                            },
                            ..Default::default()
                        };
                        helpers::set_tx_info_data_rate(&mut tx_info, &data_rate_2)?;

                        self.downlink_frame.items.push(gw::DownlinkFrameItem {
                            phy_payload: self.phy_payload.clone(),
                            tx_info: Some(tx_info),
                            tx_info_legacy: None,
                        });
                    }
                }
                _ => {
                    return Err(anyhow!("ClassMode {} is not supported", class_mode));
                }
            }
        } else {
            return Err(anyhow!("ClassMode is not set"));
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

        gateway::backend::send_downlink(
            &self.uplink_frame_set.region_config_id,
            &self.downlink_frame,
        )
        .await
        .context("Send downlink frame")?;

        Ok(())
    }
}

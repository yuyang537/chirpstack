/**
 * @module uplink/helpers
 * 
 * @description
 * 
 * # 模块概述
 * 本模块提供了ChirpStack上行数据处理中使用的各种辅助函数。这些函数不属于特定的上行消息类型
 * (如数据帧或入网请求)，而是被多个上行处理模块共享使用的通用功能。它们主要用于解析和转换
 * 上行数据的各种属性，如数据速率、信道、时间戳等，简化了上行数据处理的复杂性。
 * 
 * # 文件功能
 * - 提供上行数据速率(DR)和信道(CH)的解析和转换功能
 * - 处理上行接收时间戳的提取和转换
 * - 支持GPS时间和UTC时间的转换
 * - 提供位置信息的提取功能
 * - 设置上行调制参数的辅助函数
 * - 支持不同区域参数和LoRaWAN版本
 * 
 * # 主要组件
 * - get_uplink_dr：从上行TX信息中提取数据速率
 * - get_uplink_ch：从频率和数据速率获取上行信道
 * - get_rx_timestamp：获取接收时间戳
 * - get_time_since_gps_epoch：计算自GPS纪元以来的时间
 * - get_start_location：从接收信息中提取位置
 * - set_uplink_modulation：设置上行调制参数
 * 
 * # 关键流程
 * - 数据速率解析流程：
 *   1. 从上行TX信息中提取调制参数
 *   2. 根据调制类型(LoRa/FSK/LR-FHSS)构建数据速率结构
 *   3. 使用区域配置将数据速率结构映射到数据速率索引
 * - 时间戳处理流程：
 *   1. 从多个接收信息中选择最早的时间戳
 *   2. 根据需要转换为不同的时间格式(SystemTime/DateTime)
 *   3. 支持GPS时间和UTC时间的转换
 * 
 * # 重要考虑事项
 * - 这些辅助函数需要处理不同区域参数和LoRaWAN版本的差异
 * - 时间戳处理需要考虑不同网关的时间同步问题
 * - 数据速率和信道解析对于正确处理上行数据至关重要
 * - 这些函数应该高效且可靠，因为它们在上行处理的关键路径上
 * - 支持不同类型的调制方式(LoRa/FSK/LR-FHSS)
 */

#[cfg(test)]
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::gpstime::ToDateTime;
use crate::region;
use chirpstack_api::{common, gw};

pub fn get_uplink_dr(
    region_config_id: &str,
    tx_info: &chirpstack_api::gw::UplinkTxInfo,
) -> Result<u8> {
    let region_conf = region::get(region_config_id)?;
    let mod_info = tx_info
        .modulation
        .as_ref()
        .ok_or_else(|| anyhow!("modulation must not be None"))?;

    let mod_params = mod_info
        .parameters
        .as_ref()
        .ok_or_else(|| anyhow!("parameters must not be None"))?;

    let dr_modulation = match &mod_params {
        chirpstack_api::gw::modulation::Parameters::Lora(v) => {
            lrwn::region::DataRateModulation::Lora(lrwn::region::LoraDataRate {
                spreading_factor: v.spreading_factor as u8,
                bandwidth: v.bandwidth,
                coding_rate: v.code_rate().into(),
            })
        }
        chirpstack_api::gw::modulation::Parameters::Fsk(v) => {
            lrwn::region::DataRateModulation::Fsk(lrwn::region::FskDataRate {
                bitrate: v.datarate,
            })
        }
        chirpstack_api::gw::modulation::Parameters::LrFhss(v) => {
            lrwn::region::DataRateModulation::LrFhss(lrwn::region::LrFhssDataRate {
                coding_rate: v.code_rate().into(),
                occupied_channel_width: v.operating_channel_width,
            })
        }
    };

    region_conf.get_data_rate_index(true, &dr_modulation)
}

pub fn get_uplink_ch(region_config_id: &str, frequency: u32, dr: u8) -> Result<usize> {
    let region_conf = region::get(region_config_id)?;
    region_conf.get_uplink_channel_index_for_freq_dr(frequency, dr)
}

pub fn get_rx_timestamp(rx_info: &[gw::UplinkRxInfo]) -> SystemTime {
    // First search for time_since_gps_epoch.
    for rxi in rx_info {
        if let Some(gps_time) = &rxi.time_since_gps_epoch {
            if let Ok(ts) = chrono::Duration::from_std(Duration::new(
                gps_time.seconds as u64,
                gps_time.nanos as u32,
            )) {
                return ts.to_date_time().into();
            }
        }
    }

    // Then search for time.
    for rxi in rx_info {
        if let Some(ts) = &rxi.gw_time {
            let ts: Result<DateTime<Utc>> = (*ts).try_into().map_err(anyhow::Error::msg);
            if let Ok(ts) = ts {
                return ts.into();
            }
        }
    }

    // last resort use systemtime of NS
    SystemTime::now()
}

pub fn get_rx_timestamp_chrono(rx_info: &[gw::UplinkRxInfo]) -> DateTime<Utc> {
    // First search for time_since_gps_epoch.
    for rxi in rx_info {
        if let Some(gps_time) = &rxi.time_since_gps_epoch {
            if let Ok(ts) = chrono::Duration::from_std(Duration::new(
                gps_time.seconds as u64,
                gps_time.nanos as u32,
            )) {
                return ts.to_date_time();
            }
        }
    }

    // Then search for time.
    for rxi in rx_info {
        if let Some(ts) = &rxi.gw_time {
            let ts: Result<DateTime<Utc>> = (*ts).try_into().map_err(anyhow::Error::msg);
            if let Ok(ts) = ts {
                return ts;
            }
        }
    }

    // last resort use systemtime of NS
    Utc::now()
}

pub fn get_time_since_gps_epoch(rx_info: &[gw::UplinkRxInfo]) -> Option<Duration> {
    for rxi in rx_info {
        if let Some(gps_time) = &rxi.time_since_gps_epoch {
            return Some(Duration::new(
                gps_time.seconds as u64,
                gps_time.nanos as u32,
            ));
        }
    }

    None
}

pub fn get_time_since_gps_epoch_chrono(rx_info: &[gw::UplinkRxInfo]) -> Option<chrono::Duration> {
    for rxi in rx_info {
        if let Some(gps_time) = &rxi.time_since_gps_epoch {
            return Some(
                chrono::Duration::try_seconds(gps_time.seconds).unwrap_or_default()
                    + chrono::Duration::nanoseconds(gps_time.nanos as i64),
            );
        }
    }

    None
}

pub fn get_start_location(rx_info: &[gw::UplinkRxInfo]) -> Option<common::Location> {
    let mut with_loc: Vec<gw::UplinkRxInfo> = rx_info
        .iter()
        .filter(|&i| i.location.is_some())
        .cloned()
        .collect();
    with_loc.sort_by(|a, b| a.snr.partial_cmp(&b.snr).unwrap());
    with_loc.first().map(|i| *i.location.as_ref().unwrap())
}

#[cfg(test)]
pub fn set_uplink_modulation(
    region_config_id: &str,
    tx_info: &mut chirpstack_api::gw::UplinkTxInfo,
    dr: u8,
) -> Result<()> {
    let region_conf = region::get(region_config_id)?;
    let params = region_conf.get_data_rate(dr)?;

    tx_info.modulation = Some(gw::Modulation {
        parameters: Some(match params {
            lrwn::region::DataRateModulation::Lora(v) => {
                gw::modulation::Parameters::Lora(gw::LoraModulationInfo {
                    bandwidth: v.bandwidth,
                    spreading_factor: v.spreading_factor as u32,
                    code_rate: gw::CodeRate::from_str(&v.coding_rate)
                        .map_err(|e| anyhow!("{}", e))?
                        .into(),
                    code_rate_legacy: "".into(),
                    polarization_inversion: true,
                    no_crc: false,
                    preamble: 0,
                })
            }
            lrwn::region::DataRateModulation::Fsk(v) => {
                gw::modulation::Parameters::Fsk(gw::FskModulationInfo {
                    datarate: v.bitrate,
                    ..Default::default()
                })
            }
            lrwn::region::DataRateModulation::LrFhss(v) => {
                gw::modulation::Parameters::LrFhss(gw::LrFhssModulationInfo {
                    operating_channel_width: v.occupied_channel_width,
                    code_rate: gw::CodeRate::from_str(&v.coding_rate)
                        .map_err(|e| anyhow!("{}", e))?
                        .into(),
                    // GridSteps: this value can't be derived from a DR?
                    ..Default::default()
                })
            }
        }),
    });

    Ok(())
}

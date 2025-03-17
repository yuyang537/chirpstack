/**
 * @module region
 * 
 * @description
 * 
 * # 模块概述
 * 本模块负责管理ChirpStack系统中的LoRaWAN区域配置。LoRaWAN规范定义了不同地理区域的
 * 频率计划和参数，如EU868、US915、AS923等。本模块提供了这些区域配置的管理和访问功能。
 * 
 * # 文件功能
 * - 初始化和配置LoRaWAN区域参数
 * - 管理区域配置的全局注册表
 * - 提供区域配置的查询和访问接口
 * - 支持自定义区域参数，如额外信道和启用的上行信道
 * 
 * # 主要组件
 * - REGIONS：全局区域配置注册表
 * - setup：初始化区域配置的函数
 * - reset：重置区域配置的函数
 * - get：获取特定区域配置的函数
 * - set：设置特定区域配置的函数
 * 
 * # 关键流程
 * - 从配置文件加载区域设置
 * - 为每个启用的区域创建和配置区域对象
 * - 添加额外的信道和设置启用的上行信道
 * - 将配置好的区域对象注册到全局注册表
 * - 提供接口以通过区域ID访问区域配置
 * 
 * # 重要考虑事项
 * - 区域配置对LoRaWAN网络的性能和合规性至关重要
 * - 不同区域有不同的频率和参数要求，必须正确配置
 * - 支持自定义配置，以适应特定部署需求
 * - 区域配置是线程安全的，支持并发访问
 */

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use tracing::{info, span, trace, Level};

use crate::config;
use lrwn::region;

lazy_static! {
    static ref REGIONS: RwLock<HashMap<String, Arc<Box<dyn region::Region + Sync + Send>>>> =
        RwLock::new(HashMap::new());
}

pub fn setup() -> Result<()> {
    info!("Setting up regions");
    let conf = config::get();

    reset();

    for r in &conf.regions {
        let span = span!(Level::INFO, "setup", common_name = %r.common_name, region_id = %r.id);
        let _guard = span.enter();

        if !conf.network.enabled_regions.contains(&r.id) {
            continue;
        }

        info!("Configuring region");

        let mut region_conf = region::get(
            r.common_name,
            r.network.repeater_compatible,
            r.network.dwell_time_400ms,
        );

        for ec in &r.network.extra_channels {
            trace!(
                frequency = ec.frequency,
                min_dr = ec.min_dr,
                max_dr = ec.max_dr,
                "Adding extra channel"
            );
            region_conf
                .add_channel(ec.frequency, ec.min_dr, ec.max_dr)
                .context("Add channel")?;
        }

        if !r.network.enabled_uplink_channels.is_empty() {
            trace!("Disabling all channels first");
            for i in region_conf.get_enabled_uplink_channel_indices() {
                region_conf.disable_uplink_channel_index(i)?;
            }

            trace!(channels = ?r.network.enabled_uplink_channels, "Enabling channels");
            for i in &r.network.enabled_uplink_channels {
                region_conf.enable_uplink_channel_index(*i)?;
            }
        }

        set(&r.id, region_conf);
    }

    Ok(())
}

fn reset() {
    let mut regions_w = REGIONS.write().unwrap();
    regions_w.clear();
}

pub fn set(region_config_id: &str, r: Box<dyn region::Region + Sync + Send>) {
    let mut regions_w = REGIONS.write().unwrap();
    regions_w.insert(region_config_id.to_string(), Arc::new(r));
}

pub fn get(region_config_id: &str) -> Result<Arc<Box<dyn region::Region + Sync + Send>>> {
    let regions_r = REGIONS.read().unwrap();
    Ok(regions_r
        .get(region_config_id)
        .ok_or_else(|| {
            anyhow!(
                "region_config_id {} does not exist in REGIONS",
                region_config_id
            )
        })?
        .clone())
}

/// This returns the (first) region-name, based on the given common-name.
/// This function is used for roaming, as within the context of roaming, only
/// the common-name is given by the other party.
pub fn get_region_config_id(common_name: region::CommonName) -> Result<String> {
    let regions_r = REGIONS.read().unwrap();
    for (k, v) in &*regions_r {
        if v.get_name() == common_name {
            return Ok(k.clone());
        }
    }

    Err(anyhow!(
        "No region configured with common-name: {}",
        common_name
    ))
}

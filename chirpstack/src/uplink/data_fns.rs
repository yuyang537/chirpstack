/**
 * @module uplink/data_fns
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统中与漫游相关的上行数据处理功能。它专注于处理被动漫游(Passive Roaming)
 * 场景下的上行数据帧，使得设备可以通过外部网络服务器发送数据到ChirpStack。被动漫游是LoRaWAN协议
 * 中的一个重要特性，允许设备在不同网络服务器之间漫游，提高了网络覆盖范围和可靠性。
 * 
 * # 文件功能
 * - 处理来自外部网络的漫游设备上行数据
 * - 实现LoRaWAN后端接口规范中的被动漫游功能
 * - 转发上行数据到相应的网络服务器
 * - 管理漫游设备会话
 * - 与外部网络服务器进行通信和协商
 * - 保存和更新漫游设备的会话信息
 * 
 * # 主要组件
 * - Data结构体：处理漫游上行数据的主要结构，包含处理过程中的所有状态和数据
 * - handle方法：处理漫游上行数据的入口点
 * - start_pr_session方法：启动被动漫游会话
 * - forward_uplink_for_sessions方法：转发上行数据到相应的网络服务器
 * - save_pr_device_sessions方法：保存漫游设备会话信息
 * 
 * # 关键流程
 * - 漫游上行数据处理流程：
 *   1. 接收上行数据帧
 *   2. 过滤接收信息，只保留公共网关的信息
 *   3. 获取漫游设备会话信息
 *   4. 启动漫游会话，与外部网络服务器通信
 *   5. 转发上行数据到相应的网络服务器
 *   6. 保存更新后的漫游设备会话信息
 * 
 * # 重要考虑事项
 * - 漫游功能需要正确配置网络标识符(NetID)
 * - 与外部网络服务器的通信需要安全的通道
 * - 漫游设备会话的管理对于持续的漫游支持至关重要
 * - 需要处理不同LoRaWAN版本和区域参数的差异
 * - 漫游功能的性能和可靠性直接影响跨网络的设备通信
 * - 需要遵循LoRaWAN后端接口规范中的漫游协议
 */

use anyhow::Result;
use chrono::{Duration, Utc};
use tracing::{error, info, span, trace, Instrument, Level};
use uuid::Uuid;

use super::{error::Error, filter_rx_info_by_public_only, UplinkFrameSet};
use crate::api::backend::get_async_receiver;
use crate::backend::{keywrap, roaming};
use crate::helpers::errors::PrintFullError;
use crate::storage::passive_roaming;
use crate::uplink::helpers;
use chirpstack_api::internal;
use lrwn::NetID;

pub struct Data {
    uplink_frame_set: UplinkFrameSet,
    mac_payload: lrwn::MACPayload,
    pr_device_sessions: Vec<internal::PassiveRoamingDeviceSession>,
}

impl Data {
    pub async fn handle(ufs: UplinkFrameSet, mac_pl: lrwn::MACPayload) {
        let span = span!(Level::INFO, "data_pr");
        if let Err(e) = Data::_handle(ufs, mac_pl).instrument(span).await {
            match e.downcast_ref::<Error>() {
                Some(Error::Abort) => {
                    // nothing to do
                }
                Some(_) | None => {
                    error!(error = %e.full(), "Handle passive-roaming uplink error");
                }
            }
        }
    }

    async fn _handle(ufs: UplinkFrameSet, mac_pl: lrwn::MACPayload) -> Result<()> {
        let mut ctx = Data {
            uplink_frame_set: ufs,
            mac_payload: mac_pl,
            pr_device_sessions: Vec::new(),
        };

        ctx.filter_rx_info_by_public_only()?;
        ctx.get_pr_device_sessions().await?;
        ctx.start_pr_sessions().await?;
        ctx.forward_uplink_for_sessions().await?;
        ctx.save_pr_device_sessions().await?;

        Ok(())
    }

    fn filter_rx_info_by_public_only(&mut self) -> Result<()> {
        trace!("Filtering rx_info by public gateways only");
        filter_rx_info_by_public_only(&mut self.uplink_frame_set)?;
        Ok(())
    }

    async fn get_pr_device_sessions(&mut self) -> Result<()> {
        trace!("Getting passive-roaming device-sessions");
        self.pr_device_sessions =
            passive_roaming::get_for_phy_payload(&self.uplink_frame_set.phy_payload).await?;

        for ds in &mut self.pr_device_sessions {
            ds.f_cnt_up = self.mac_payload.fhdr.f_cnt + 1;
        }

        trace!(
            count = self.pr_device_sessions.len(),
            "Got passive-roaming device-sessions"
        );

        Ok(())
    }

    async fn start_pr_sessions(&mut self) -> Result<()> {
        // Skip this step when we already have active sessions.
        if !self.pr_device_sessions.is_empty() {
            return Ok(());
        }

        let net_ids = roaming::get_net_ids_for_dev_addr(self.mac_payload.fhdr.devaddr);

        trace!(net_ids = ?net_ids, "Got NetIDs");

        for net_id in net_ids {
            let ds = match self.start_pr_session(net_id).await {
                Ok(v) => v,
                Err(e) => {
                    error!(net_id = %net_id, error = %e.full(), "Start passive-roaming error");
                    continue;
                }
            };

            // No need to store the device-session or call XmitDataReq when
            // lifetime is not set (stateless passive-roaming).
            if ds.lifetime.is_some() {
                self.pr_device_sessions.push(ds);
            }
        }

        Ok(())
    }

    async fn forward_uplink_for_sessions(&self) -> Result<()> {
        trace!("Forwarding uplink for passive-roaming sessions");

        for ds in &self.pr_device_sessions {
            let mut req = backend::XmitDataReqPayload {
                phy_payload: self.uplink_frame_set.phy_payload.to_vec()?,
                ul_meta_data: Some(backend::ULMetaData {
                    dev_addr: self.mac_payload.fhdr.devaddr.to_vec(),
                    data_rate: Some(self.uplink_frame_set.dr),
                    ul_freq: Some((self.uplink_frame_set.tx_info.frequency as f64) / 1_000_000.0),
                    recv_time: helpers::get_rx_timestamp_chrono(&self.uplink_frame_set.rx_info_set),
                    rf_region: self
                        .uplink_frame_set
                        .region_common_name
                        .to_string()
                        .replace('_', "-"),
                    gw_cnt: Some(self.uplink_frame_set.rx_info_set.len()),
                    gw_info: roaming::rx_info_to_gw_info(&self.uplink_frame_set.rx_info_set)?,
                    ..Default::default()
                }),
                ..Default::default()
            };

            let net_id = NetID::from_slice(&ds.net_id)?;
            let client = roaming::get(&net_id).await?;
            let async_receiver = match client.is_async() {
                false => None,
                true => Some(
                    get_async_receiver(req.base.transaction_id, client.get_async_timeout()).await?,
                ),
            };

            if let Err(e) = client
                .xmit_data_req(backend::Role::SNS, &mut req, async_receiver)
                .await
            {
                error!(net_id = %net_id, error = %e.full(), "XmitDataReq failed");
            }
        }

        Ok(())
    }

    async fn save_pr_device_sessions(&self) -> Result<()> {
        trace!("Saving passive-roaming device-sessions");

        for ds in &self.pr_device_sessions {
            passive_roaming::save(ds).await?;
        }

        Ok(())
    }

    async fn start_pr_session(
        &self,
        net_id: NetID,
    ) -> Result<internal::PassiveRoamingDeviceSession> {
        info!(net_id = %net_id, dev_addr = %self.mac_payload.fhdr.devaddr, "Starting passive-roaming session");

        let mut pr_req = backend::PRStartReqPayload {
            phy_payload: self.uplink_frame_set.phy_payload.to_vec()?,
            ul_meta_data: backend::ULMetaData {
                ul_freq: Some((self.uplink_frame_set.tx_info.frequency as f64) / 1_000_000.0),
                data_rate: Some(self.uplink_frame_set.dr),
                recv_time: helpers::get_rx_timestamp_chrono(&self.uplink_frame_set.rx_info_set),
                rf_region: self
                    .uplink_frame_set
                    .region_common_name
                    .to_string()
                    .replace('_', "-"),
                gw_cnt: Some(self.uplink_frame_set.rx_info_set.len()),
                gw_info: roaming::rx_info_to_gw_info(&self.uplink_frame_set.rx_info_set)?,
                ..Default::default()
            },
            ..Default::default()
        };

        #[cfg(test)]
        {
            pr_req.base.transaction_id = 1234;
        }

        let client = roaming::get(&net_id).await?;
        let async_receiver = match client.is_async() {
            false => None,
            true => Some(
                get_async_receiver(pr_req.base.transaction_id, client.get_async_timeout()).await?,
            ),
        };

        let pr_start_ans = client
            .pr_start_req(backend::Role::SNS, &mut pr_req, async_receiver)
            .await?;
        let sess_id = Uuid::new_v4();

        Ok(internal::PassiveRoamingDeviceSession {
            session_id: sess_id.as_bytes().to_vec(),
            net_id: net_id.to_vec(),
            dev_addr: self.mac_payload.fhdr.devaddr.to_vec(),
            validate_mic: roaming::get_passive_roaming_validate_mic(net_id)?,
            lifetime: {
                let lt = pr_start_ans.lifetime.unwrap_or_default() as i64;
                if lt == 0 {
                    None
                } else {
                    Some((Utc::now() + Duration::try_seconds(lt).unwrap_or_default()).into())
                }
            },
            f_nwk_s_int_key: match &pr_start_ans.f_nwk_s_int_key {
                Some(ke) => keywrap::unwrap(ke)?.to_vec(),
                None => match &pr_start_ans.nwk_s_key {
                    None => Vec::new(),
                    Some(ke) => keywrap::unwrap(ke)?.to_vec(),
                },
            },
            f_cnt_up: pr_start_ans.f_cnt_up.unwrap_or_default(),
            ..Default::default()
        })
    }
}

/**
 * @module uplink/join_fns
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统中与漫游相关的入网请求处理功能。它专注于处理被动漫游(Passive Roaming)
 * 场景下的LoRaWAN入网请求，使得设备可以通过外部网络服务器加入到其归属网络。这是LoRaWAN协议中
 * 支持设备跨网络漫游的关键组件，确保设备即使在非归属网络的覆盖范围内也能成功入网。
 * 
 * # 文件功能
 * - 处理来自漫游设备的入网请求
 * - 确定设备的归属网络(Home NetID)
 * - 与外部Join Server和网络服务器通信
 * - 启动被动漫游会话
 * - 转发入网请求到相应的归属网络服务器
 * - 保存漫游会话信息
 * - 处理入网响应并转发给设备
 * 
 * # 主要组件
 * - JoinRequest结构体：处理漫游入网请求的主要结构，包含处理过程中的所有状态和数据
 * - start_pr方法：启动被动漫游入网请求处理的入口点
 * - get_home_net_id方法：确定设备的归属网络标识符
 * - start_roaming方法：启动与归属网络的漫游通信
 * - save_roaming_session方法：保存漫游会话信息
 * 
 * # 关键流程
 * - 漫游入网请求处理流程：
 *   1. 接收入网请求帧
 *   2. 过滤接收信息，只保留公共网关的信息
 *   3. 确定设备的归属网络标识符
 *   4. 获取与归属网络通信的客户端
 *   5. 启动漫游过程，发送PRStartReq消息
 *   6. 处理PRStartAns响应
 *   7. 保存漫游会话信息
 * 
 * # 重要考虑事项
 * - 漫游入网功能需要正确配置网络标识符(NetID)和Join EUI前缀
 * - 与外部网络服务器的通信需要安全的通道和正确的认证
 * - 漫游会话的管理对于后续的数据通信至关重要
 * - 需要处理不同LoRaWAN版本和区域参数的差异
 * - 入网请求的处理时效性对设备的成功入网至关重要
 * - 需要遵循LoRaWAN后端接口规范中的漫游协议
 */

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use tracing::{span, trace, Instrument, Level};
use uuid::Uuid;

use super::{filter_rx_info_by_public_only, UplinkFrameSet};
use crate::api::backend::get_async_receiver;
use crate::backend::{joinserver, keywrap, roaming};
use crate::downlink;
use crate::storage::passive_roaming;
use crate::uplink::helpers;
use backend::Client;
use chirpstack_api::internal;
use lrwn::{JoinRequestPayload, NetID};

pub struct JoinRequest {
    uplink_frame_set: UplinkFrameSet,
    join_request: JoinRequestPayload,
    home_net_id: Option<NetID>,
    client: Option<Arc<Client>>,
    pr_start_ans: Option<backend::PRStartAnsPayload>,
}

impl JoinRequest {
    pub async fn start_pr(ufs: UplinkFrameSet, jr: JoinRequestPayload) -> Result<()> {
        let span = span!(Level::INFO, "start_pr");
        JoinRequest::_start_pr(ufs, jr).instrument(span).await
    }

    async fn _start_pr(ufs: UplinkFrameSet, jr: JoinRequestPayload) -> Result<()> {
        let mut ctx = JoinRequest {
            uplink_frame_set: ufs,
            join_request: jr,
            home_net_id: None,
            client: None,
            pr_start_ans: None,
        };

        ctx.filter_rx_info_by_public_only()?;
        ctx.get_home_net_id().await?;
        ctx.get_client().await?;
        ctx.start_roaming().await?;
        ctx.save_roaming_session().await?;

        Ok(())
    }

    fn filter_rx_info_by_public_only(&mut self) -> Result<()> {
        trace!("Filtering rx_info by public gateways only");
        filter_rx_info_by_public_only(&mut self.uplink_frame_set)?;

        Ok(())
    }

    async fn get_home_net_id(&mut self) -> Result<()> {
        trace!("Getting home netid");

        trace!(join_eui = %self.join_request.join_eui, "Trying to get join-server client");
        let js_client = joinserver::get(self.join_request.join_eui).await?;

        let mut home_ns_req = backend::HomeNSReqPayload {
            dev_eui: self.join_request.dev_eui.to_vec(),
            ..Default::default()
        };

        #[cfg(test)]
        {
            home_ns_req.base.transaction_id = 1234;
        }

        let async_receiver = match js_client.is_async() {
            false => None,
            true => Some(
                get_async_receiver(
                    home_ns_req.base.transaction_id,
                    js_client.get_async_timeout(),
                )
                .await?,
            ),
        };

        trace!("Requesting home netid");
        let home_ns_ans = js_client
            .home_ns_req(
                self.join_request.join_eui.to_vec(),
                &mut home_ns_req,
                async_receiver,
            )
            .await?;
        self.home_net_id = Some(NetID::from_slice(&home_ns_ans.h_net_id)?);

        Ok(())
    }

    async fn get_client(&mut self) -> Result<()> {
        let net_id = self.home_net_id.as_ref().unwrap();
        trace!(net_id = %net_id, "Getting backend interfaces client");
        self.client = Some(roaming::get(net_id).await?);
        Ok(())
    }

    async fn start_roaming(&mut self) -> Result<()> {
        trace!("Starting passive-roaming");

        let mut pr_req = backend::PRStartReqPayload {
            phy_payload: self.uplink_frame_set.phy_payload.to_vec()?,
            ul_meta_data: backend::ULMetaData {
                dev_eui: self.join_request.dev_eui.to_vec(),
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

        let client = self.client.as_ref().unwrap();
        let async_receiver = match client.is_async() {
            false => None,
            true => Some(
                get_async_receiver(pr_req.base.transaction_id, client.get_async_timeout()).await?,
            ),
        };

        let resp = client
            .pr_start_req(backend::Role::SNS, &mut pr_req, async_receiver)
            .await?;

        if let Some(dl_meta) = &resp.dl_meta_data {
            downlink::roaming::PassiveRoamingDownlink::handle(
                self.uplink_frame_set.clone(),
                resp.phy_payload.clone(),
                dl_meta.clone(),
            )
            .await?;
        } else {
            return Err(anyhow!("DLMetaData is not set"));
        }

        self.pr_start_ans = Some(resp);
        Ok(())
    }

    async fn save_roaming_session(&mut self) -> Result<()> {
        trace!("Saving roaming-session");

        let pr_start_ans = self.pr_start_ans.as_ref().unwrap();

        if pr_start_ans.dev_addr.is_empty()
            || pr_start_ans.lifetime.is_none()
            || pr_start_ans.lifetime.unwrap() == 0
        {
            return Ok(());
        }

        let sess_id = Uuid::new_v4();

        let sess = internal::PassiveRoamingDeviceSession {
            session_id: sess_id.as_bytes().to_vec(),
            net_id: self.home_net_id.unwrap().to_vec(),
            validate_mic: roaming::get_passive_roaming_validate_mic(self.home_net_id.unwrap())?,
            dev_addr: pr_start_ans.dev_addr.clone(),
            dev_eui: self.join_request.dev_eui.to_vec(),
            lifetime: {
                let lt = pr_start_ans.lifetime.unwrap_or_default() as i64;
                if lt == 0 {
                    None
                } else {
                    Some((Utc::now() + Duration::try_seconds(lt).unwrap_or_default()).into())
                }
            },
            lorawan_1_1: pr_start_ans.f_nwk_s_int_key.is_some(),

            f_nwk_s_int_key: match &pr_start_ans.f_nwk_s_int_key {
                Some(ke) => keywrap::unwrap(ke)?.to_vec(),
                None => match &pr_start_ans.nwk_s_key {
                    None => Vec::new(),
                    Some(ke) => keywrap::unwrap(ke)?.to_vec(),
                },
            },
            ..Default::default()
        };

        passive_roaming::save(&sess)
            .await
            .context("Save passive-roaming device-session")
    }
}

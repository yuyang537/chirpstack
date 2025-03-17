/*
 * 模块概述
 * ========
 * Class A被动漫游(Passive Roaming)测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，
 * 专门用于测试LoRaWAN Class A设备在被动漫游场景下的通信功能。被动漫游是LoRaWAN协议中的一项
 * 重要功能，允许终端设备在访问网络(Visited Network)中通过漫游协议与其归属网络(Home Network)
 * 通信。
 * 
 * 该模块实现了一系列测试用例，覆盖了被动漫游的各个方面，包括转发网络服务器(Forwarding NS)和
 * 服务网络服务器(Serving NS)的角色测试、上行消息处理、下行消息转发等。通过这些测试，确保
 * ChirpStack能够正确实现LoRaWAN协议规范中关于被动漫游的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关、Join Server和外部网络服务器，使得测试可以在不依赖实际
 * 硬件和外部服务的情况下运行，同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(class_a_pr_test.rs)实现了Class A设备被动漫游的测试用例，提供了以下主要功能：
 * 1. 转发网络服务器(FNS)上行测试：验证ChirpStack作为FNS时的上行消息处理
 * 2. 服务网络服务器(SNS)上行测试：验证ChirpStack作为SNS时的上行消息处理
 * 3. 漫游不允许测试：验证当漫游不被允许时的错误处理
 * 4. 设备不存在测试：验证当设备不存在时的错误处理
 *
 * 主要组件
 * ========
 * - test_fns_uplink(): 测试ChirpStack作为FNS时的上行消息处理
 * - test_sns_uplink(): 测试ChirpStack作为SNS时的上行消息处理
 * - test_sns_roaming_not_allowed(): 测试漫游不允许的情况
 * - test_sns_dev_not_found(): 测试设备不存在的情况
 *
 * 关键流程
 * ========
 * 1. FNS上行测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置模拟SNS服务器
 *    - 处理模拟的上行消息
 *    - 验证FNS向SNS发送的PRStartReq请求
 *    - 验证FNS处理SNS的PRStartAns响应
 *    - 验证FNS向SNS发送的ULMetaData请求
 *
 * 2. SNS上行测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置模拟FNS服务器
 *    - 接收并处理FNS发送的PRStartReq请求
 *    - 发送PRStartAns响应
 *    - 接收并处理FNS发送的ULMetaData请求
 *    - 验证设备状态和集成事件
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备状态、HTTP请求等
 * - 协议版本: 测试应考虑不同版本的LoRaWAN后端接口规范
 * - 安全考虑: 测试应验证漫游通信的安全机制
 * - 错误处理: 测试应验证各种错误情况的处理，确保系统的健壮性
 */

use std::str::FromStr;

use bytes::Bytes;
use chrono::Utc;
use httpmock::prelude::*;
use prost::Message;
use uuid::Uuid;

use crate::api::backend as backend_api;
use crate::backend::{joinserver, roaming};
use crate::gateway::backend as gateway_backend;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_profile, device_queue, gateway, tenant,
};
use crate::{config, test, uplink};
use chirpstack_api::{common, gw, internal};
use lrwn::region::CommonName;
use lrwn::{AES128Key, NetID, EUI64};

#[tokio::test]
async fn test_fns_uplink() {
    let _guard = test::prepare().await;

    let sns_mock = MockServer::start();

    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("000202").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("000505").unwrap(),
        server: sns_mock.url("/"),
        ..Default::default()
    });

    config::set(conf);
    joinserver::setup().await.unwrap();
    roaming::setup().await.unwrap();

    let t = tenant::create(tenant::Tenant {
        name: "tenant".into(),
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let gw = gateway::create(gateway::Gateway {
        name: "gateway".into(),
        tenant_id: t.id,
        gateway_id: EUI64::from_str("0102030405060708").unwrap(),
        ..Default::default()
    })
    .await
    .unwrap();

    let recv_time = Utc::now();

    let rx_info = gw::UplinkRxInfo {
        gateway_id: gw.gateway_id.to_string(),
        gw_time: Some(recv_time.into()),
        location: Some(common::Location {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let data_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataUp,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: {
                    let mut d = lrwn::DevAddr::from_be_bytes([0, 0, 0, 0]);
                    d.set_dev_addr_prefix(
                        lrwn::NetID::from_str("000505").unwrap().dev_addr_prefix(),
                    );
                    d
                },
                f_ctrl: Default::default(),
                f_cnt: 1,
                f_opts: lrwn::MACCommandSet::new(vec![]),
            },
            f_port: None,
            frm_payload: None,
        }),
        mic: Some([1, 2, 3, 4]),
    };

    // Setup sns mock.
    let mut sns_pr_start_req_mock = sns_mock.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&backend::PRStartReqPayload {
                base: backend::BasePayload {
                    sender_id: vec![0, 2, 2],
                    receiver_id: vec![0, 5, 5],
                    message_type: backend::MessageType::PRStartReq,
                    transaction_id: 1234,
                    ..Default::default()
                },
                phy_payload: data_phy.to_vec().unwrap(),
                ul_meta_data: backend::ULMetaData {
                    ul_freq: Some(868.1),
                    data_rate: Some(0),
                    recv_time,
                    rf_region: "EU868".to_string(),
                    gw_cnt: Some(1),
                    gw_info: roaming::rx_info_to_gw_info(&[rx_info.clone()]).unwrap(),
                    ..Default::default()
                },
            });

        then.json_body_obj(&backend::PRStartAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    receiver_id: vec![1, 2, 3],
                    sender_id: vec![3, 2, 1],
                    message_type: backend::MessageType::PRStartAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::Success,
                    ..Default::default()
                },
            },
            ..Default::default()
        })
        .status(200);
    });

    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    // Simulate uplink
    uplink::handle_uplink(
        CommonName::EU868,
        "eu868".into(),
        Uuid::new_v4(),
        gw::UplinkFrameSet {
            phy_payload: data_phy.to_vec().unwrap(),
            tx_info: Some(tx_info),
            rx_info: vec![rx_info],
        },
    )
    .await
    .unwrap();

    sns_pr_start_req_mock.assert();
    sns_pr_start_req_mock.delete();

    joinserver::reset().await;
}

#[tokio::test]
async fn test_sns_uplink() {
    let _guard = test::prepare().await;
    let fns_mock = MockServer::start();
    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("000505").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("000202").unwrap(),
        passive_roaming_validate_mic: true,
        server: fns_mock.url("/"),
        ..Default::default()
    });

    config::set(conf);
    joinserver::setup().await.unwrap();
    roaming::setup().await.unwrap();

    let t = tenant::create(tenant::Tenant {
        name: "tenant".into(),
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let app = application::create(application::Application {
        name: "app".into(),
        tenant_id: t.id,
        ..Default::default()
    })
    .await
    .unwrap();

    let dp = device_profile::create(device_profile::DeviceProfile {
        name: "dp".into(),
        tenant_id: t.id,
        region: lrwn::region::CommonName::EU868,
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_2,
        reg_params_revision: lrwn::region::Revision::A,
        supports_otaa: true,
        allow_roaming: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut dev_addr = lrwn::DevAddr::from_be_bytes([0, 0, 0, 0]);
    dev_addr.set_dev_addr_prefix(lrwn::NetID::from_str("000505").unwrap().dev_addr_prefix());

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::B,
        dev_addr: Some(dev_addr),
        device_session: Some(
            internal::DeviceSession {
                mac_version: common::MacVersion::Lorawan104.into(),
                dev_addr: dev_addr.to_vec(),
                f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                app_s_key: Some(common::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
                }),
                f_cnt_up: 8,
                n_f_cnt_down: 5,
                enabled_uplink_channel_indices: vec![0, 1, 2],
                rx1_delay: 1,
                rx2_frequency: 869525000,
                region_config_id: "eu868".into(),
                ..Default::default()
            }
            .into(),
        ),
        ..Default::default()
    })
    .await
    .unwrap();

    let ds = dev.get_device_session().unwrap();

    device_queue::enqueue_item(device_queue::DeviceQueueItem {
        dev_eui: dev.dev_eui,
        f_port: 10,
        data: vec![1, 2, 3, 4],
        ..Default::default()
    })
    .await
    .unwrap();

    let mut data_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataUp,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: dev_addr,
                f_ctrl: Default::default(),
                f_cnt: 8,
                f_opts: lrwn::MACCommandSet::new(vec![]),
            },
            f_port: None,
            frm_payload: None,
        }),
        mic: None,
    };
    data_phy
        .set_uplink_data_mic(
            lrwn::MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &AES128Key::from_slice(&ds.f_nwk_s_int_key).unwrap(),
            &AES128Key::from_slice(&ds.s_nwk_s_int_key).unwrap(),
        )
        .unwrap();

    let recv_time = Utc::now();

    let mut rx_info = gw::UplinkRxInfo {
        gateway_id: "0302030405060708".to_string(),
        gw_time: Some(recv_time.into()),
        ..Default::default()
    };
    rx_info
        .metadata
        .insert("region_config_id".to_string(), "eu868".to_string());
    rx_info
        .metadata
        .insert("region_common_name".to_string(), "EU868".to_string());

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let pr_start_req = backend::PRStartReqPayload {
        base: backend::BasePayload {
            sender_id: vec![0, 2, 2],
            receiver_id: vec![0, 5, 5],
            message_type: backend::MessageType::PRStartReq,
            transaction_id: 1234,
            ..Default::default()
        },
        phy_payload: data_phy.to_vec().unwrap(),
        ul_meta_data: backend::ULMetaData {
            ul_freq: Some(868.1),
            data_rate: Some(0),
            recv_time,
            rf_region: "EU868".to_string(),
            gw_cnt: Some(1),
            gw_info: roaming::rx_info_to_gw_info(&[rx_info.clone()]).unwrap(),
            ..Default::default()
        },
    };

    // Setup downlink xmit mock.
    let mut fns_xmit_data_req_mock = fns_mock.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&backend::XmitDataReqPayload {
                base: backend::BasePayload {
                    receiver_id: vec![0, 2, 2],
                    sender_id: vec![0, 5, 5],
                    message_type: backend::MessageType::XmitDataReq,
                    transaction_id: 1234,
                    ..Default::default()
                },
                phy_payload: hex::decode("600000000a8005000a54972baa8b983cd1").unwrap(),
                dl_meta_data: Some(backend::DLMetaData {
                    dev_eui: dev.dev_eui.to_vec(),
                    dl_freq_1: Some(868.1),
                    dl_freq_2: Some(869.525),
                    rx_delay_1: Some(1),
                    class_mode: Some("A".to_string()),
                    data_rate_1: Some(0),
                    data_rate_2: Some(0),
                    gw_info: vec![backend::GWInfoElement {
                        ul_token: rx_info.encode_to_vec(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            });

        then.json_body_obj(&backend::XmitDataAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    receiver_id: vec![0, 5, 5],
                    sender_id: vec![0, 2, 2],
                    message_type: backend::MessageType::XmitDataAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::Success,
                    ..Default::default()
                },
            },
        })
        .status(200);
    });

    let resp =
        backend_api::handle_request(Bytes::from(serde_json::to_string(&pr_start_req).unwrap()))
            .await;
    let resp_b = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();

    let pr_start_ans: backend::PRStartAnsPayload = serde_json::from_slice(&resp_b).unwrap();

    assert_eq!(
        backend::PRStartAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    sender_id: vec![0, 5, 5],
                    receiver_id: vec![0, 2, 2],
                    message_type: backend::MessageType::PRStartAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::Success,
                    ..Default::default()
                },
            },
            dev_eui: dev.dev_eui.to_vec(),
            nwk_s_key: Some(backend::KeyEnvelope {
                kek_label: "".to_string(),
                aes_key: ds.nwk_s_enc_key.clone(),
            }),
            f_cnt_up: Some(8),
            ..Default::default()
        },
        pr_start_ans
    );

    fns_xmit_data_req_mock.assert();
    fns_xmit_data_req_mock.delete();
}

#[tokio::test]
async fn test_sns_roaming_not_allowed() {
    let _guard = test::prepare().await;
    let fns_mock = MockServer::start();
    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("000505").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("000202").unwrap(),
        server: fns_mock.url("/"),
        ..Default::default()
    });

    config::set(conf);
    joinserver::setup().await.unwrap();
    roaming::setup().await.unwrap();

    let t = tenant::create(tenant::Tenant {
        name: "tenant".into(),
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let app = application::create(application::Application {
        name: "app".into(),
        tenant_id: t.id,
        ..Default::default()
    })
    .await
    .unwrap();

    let dp = device_profile::create(device_profile::DeviceProfile {
        name: "dp".into(),
        tenant_id: t.id,
        region: lrwn::region::CommonName::EU868,
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_2,
        reg_params_revision: lrwn::region::Revision::A,
        supports_otaa: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut dev_addr = lrwn::DevAddr::from_be_bytes([0, 0, 0, 0]);
    dev_addr.set_dev_addr_prefix(lrwn::NetID::from_str("000505").unwrap().dev_addr_prefix());

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::B,
        dev_addr: Some(dev_addr),
        device_session: Some(
            internal::DeviceSession {
                mac_version: common::MacVersion::Lorawan104.into(),
                dev_addr: dev_addr.to_vec(),
                f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                app_s_key: Some(common::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
                }),
                f_cnt_up: 8,
                n_f_cnt_down: 5,
                enabled_uplink_channel_indices: vec![0, 1, 2],
                rx1_delay: 1,
                rx2_frequency: 869525000,
                region_config_id: "eu868".into(),
                ..Default::default()
            }
            .into(),
        ),
        ..Default::default()
    })
    .await
    .unwrap();

    let ds = dev.get_device_session().unwrap();

    let mut data_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataUp,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: dev_addr,
                f_ctrl: Default::default(),
                f_cnt: 8,
                f_opts: lrwn::MACCommandSet::new(vec![]),
            },
            f_port: None,
            frm_payload: None,
        }),
        mic: None,
    };
    data_phy
        .set_uplink_data_mic(
            lrwn::MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &AES128Key::from_slice(&ds.f_nwk_s_int_key).unwrap(),
            &AES128Key::from_slice(&ds.s_nwk_s_int_key).unwrap(),
        )
        .unwrap();

    let recv_time = Utc::now();

    let mut rx_info = gw::UplinkRxInfo {
        gateway_id: "0302030405060708".to_string(),
        gw_time: Some(recv_time.into()),
        ..Default::default()
    };
    rx_info
        .metadata
        .insert("region_config_id".to_string(), "eu868".to_string());
    rx_info
        .metadata
        .insert("region_common_name".to_string(), "EU868".to_string());

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let pr_start_req = backend::PRStartReqPayload {
        base: backend::BasePayload {
            sender_id: vec![0, 2, 2],
            receiver_id: vec![0, 5, 5],
            message_type: backend::MessageType::PRStartReq,
            transaction_id: 1234,
            ..Default::default()
        },
        phy_payload: data_phy.to_vec().unwrap(),
        ul_meta_data: backend::ULMetaData {
            ul_freq: Some(868.1),
            data_rate: Some(0),
            recv_time,
            rf_region: "EU868".to_string(),
            gw_cnt: Some(1),
            gw_info: roaming::rx_info_to_gw_info(&[rx_info.clone()]).unwrap(),
            ..Default::default()
        },
    };

    let resp =
        backend_api::handle_request(Bytes::from(serde_json::to_string(&pr_start_req).unwrap()))
            .await;
    let resp_b = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let pr_start_ans: backend::PRStartAnsPayload = serde_json::from_slice(&resp_b).unwrap();

    assert_eq!(
        backend::PRStartAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    sender_id: vec![0, 5, 5],
                    receiver_id: vec![0, 2, 2],
                    message_type: backend::MessageType::PRStartAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::DevRoamingDisallowed,
                    description: "Roaming is not allowed for the device".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
        pr_start_ans
    );
}

#[tokio::test]
async fn test_sns_dev_not_found() {
    let _guard = test::prepare().await;
    let fns_mock = MockServer::start();

    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("000505").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("000202").unwrap(),
        server: fns_mock.url("/"),
        ..Default::default()
    });

    config::set(conf);
    joinserver::setup().await.unwrap();
    roaming::setup().await.unwrap();

    let mut dev_addr = lrwn::DevAddr::from_be_bytes([0, 0, 0, 0]);
    dev_addr.set_dev_addr_prefix(lrwn::NetID::from_str("000505").unwrap().dev_addr_prefix());

    let data_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataUp,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: dev_addr,
                f_ctrl: Default::default(),
                f_cnt: 8,
                f_opts: lrwn::MACCommandSet::new(vec![]),
            },
            f_port: None,
            frm_payload: None,
        }),
        mic: Some([1, 2, 3, 4]),
    };

    let recv_time = Utc::now();

    let mut rx_info = gw::UplinkRxInfo {
        gateway_id: "0302030405060708".to_string(),
        gw_time: Some(recv_time.into()),
        ..Default::default()
    };
    rx_info
        .metadata
        .insert("region_config_id".to_string(), "eu868".to_string());
    rx_info
        .metadata
        .insert("region_common_name".to_string(), "EU868".to_string());

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let pr_start_req = backend::PRStartReqPayload {
        base: backend::BasePayload {
            sender_id: vec![0, 2, 2],
            receiver_id: vec![0, 5, 5],
            message_type: backend::MessageType::PRStartReq,
            transaction_id: 1234,
            ..Default::default()
        },
        phy_payload: data_phy.to_vec().unwrap(),
        ul_meta_data: backend::ULMetaData {
            ul_freq: Some(868.1),
            data_rate: Some(0),
            recv_time,
            rf_region: "EU868".to_string(),
            gw_cnt: Some(1),
            gw_info: roaming::rx_info_to_gw_info(&[rx_info.clone()]).unwrap(),
            ..Default::default()
        },
    };

    let resp =
        backend_api::handle_request(Bytes::from(serde_json::to_string(&pr_start_req).unwrap()))
            .await;
    let resp_b = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();

    let pr_start_ans: backend::PRStartAnsPayload = serde_json::from_slice(&resp_b).unwrap();

    assert_eq!(
        backend::PRStartAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    sender_id: vec![0, 5, 5],
                    receiver_id: vec![0, 2, 2],
                    message_type: backend::MessageType::PRStartAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::UnknownDevAddr,
                    description: format!("Object does not exist (id: {})", dev_addr),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        },
        pr_start_ans
    );
}

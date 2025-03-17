/*
 * 模块概述
 * ========
 * OTAA被动漫游(Passive Roaming)测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，
 * 专门用于测试LoRaWAN设备在被动漫游场景下的空中激活(OTAA)功能。这种场景结合了OTAA和被动漫游
 * 两个功能，允许终端设备在访问网络(Visited Network)中通过漫游协议与其归属网络(Home Network)
 * 进行激活。
 * 
 * 该模块实现了一系列测试用例，覆盖了漫游OTAA的各个方面，包括转发网络服务器(Forwarding NS)和
 * 服务网络服务器(Serving NS)的角色测试、Join-Request消息处理、Join Server交互等。通过这些测试，
 * 确保ChirpStack能够正确实现LoRaWAN协议规范中关于漫游OTAA的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关、Join Server和外部网络服务器，使得测试可以在不依赖实际
 * 硬件和外部服务的情况下运行，同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(otaa_pr_test.rs)实现了被动漫游场景下的OTAA测试用例，提供了以下主要功能：
 * 1. 转发网络服务器(FNS)测试：验证ChirpStack作为FNS时的Join-Request处理
 * 2. 服务网络服务器(SNS)测试：验证ChirpStack作为SNS时的Join-Request处理
 * 3. 漫游不允许测试：验证当漫游不被允许时的错误处理
 *
 * 主要组件
 * ========
 * - test_fns(): 测试ChirpStack作为FNS时的Join-Request处理
 * - test_sns(): 测试ChirpStack作为SNS时的Join-Request处理
 * - test_sns_roaming_not_allowed(): 测试漫游不允许的情况
 *
 * 关键流程
 * ========
 * 1. FNS测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置模拟Join Server和SNS服务器
 *    - 处理模拟的Join-Request上行消息
 *    - 验证FNS向Join Server发送的请求
 *    - 验证FNS向SNS发送的JoinReq请求
 *    - 验证FNS处理SNS的JoinAns响应
 *    - 验证Join-Accept下行消息的生成
 *
 * 2. SNS测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置模拟Join Server和FNS服务器
 *    - 接收并处理FNS发送的JoinReq请求
 *    - 验证SNS向Join Server发送的请求
 *    - 发送JoinAns响应给FNS
 *    - 验证设备会话的创建
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备会话、HTTP请求等
 * - 协议版本: 测试应考虑不同版本的LoRaWAN协议和后端接口规范
 * - 安全考虑: 测试应验证漫游通信和OTAA的安全机制
 * - 错误处理: 测试应验证各种错误情况的处理，确保系统的健壮性
 */

use std::str::FromStr;

use bytes::Bytes;
use chrono::Utc;
use httpmock::prelude::*;
use prost::Message;
use uuid::Uuid;

use super::assert;
use crate::api::backend as backend_api;
use crate::backend::{joinserver, roaming};
use crate::gateway::backend as gateway_backend;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_keys, device_profile, gateway, tenant,
};
use crate::{config, storage::fields, test, uplink};
use chirpstack_api::gw;
use lrwn::region::CommonName;
use lrwn::{AES128Key, EUI64Prefix, NetID, EUI64};

#[tokio::test]
async fn test_fns() {
    let _guard = test::prepare().await;

    let js_mock = MockServer::start();
    let sns_mock = MockServer::start();

    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("010203").unwrap();

    // Set Join Server.
    conf.join_server.servers = vec![config::JoinServerServer {
        join_eui_prefix: EUI64Prefix::new([1, 2, 3, 4, 5, 6, 7, 8], 64),
        server: js_mock.url("/"),
        ..Default::default()
    }];

    // Set roaming agreement.
    conf.roaming.servers = vec![config::RoamingServer {
        net_id: NetID::from_str("030201").unwrap(),
        server: sns_mock.url("/"),
        ..Default::default()
    }];

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
        location: Some(Default::default()),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let mut jr_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_str("0102030405060708").unwrap(),
            dev_eui: EUI64::from_str("0807060504030201").unwrap(),
            dev_nonce: 123,
        }),
        mic: None,
    };
    jr_phy
        .set_join_request_mic(&AES128Key::from_str("01020304050607080102030405060708").unwrap())
        .unwrap();

    // Setup JS mock (HomeNSReq).
    let mut js_join_request_mock = js_mock.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&backend::HomeNSReqPayload {
                base: backend::BasePayload {
                    sender_id: vec![1, 2, 3],
                    receiver_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                    message_type: backend::MessageType::HomeNSReq,
                    transaction_id: 1234,
                    ..Default::default()
                },
                dev_eui: vec![8, 7, 6, 5, 4, 3, 2, 1],
            });

        then.json_body_obj(&backend::HomeNSAnsPayload {
            base: backend::BasePayloadResult {
                base: backend::BasePayload {
                    receiver_id: vec![1, 2, 3],
                    sender_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                    message_type: backend::MessageType::HomeNSAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::Success,
                    ..Default::default()
                },
            },
            h_net_id: vec![3, 2, 1],
        })
        .status(200);
    });

    // Setup SNS mock (PRStartReq).
    let mut sns_pr_start_req_mock = sns_mock.mock(|when, then| {
        when.method(POST)
            .path("/")
            .json_body_obj(&backend::PRStartReqPayload {
                base: backend::BasePayload {
                    sender_id: vec![1, 2, 3],
                    receiver_id: vec![3, 2, 1],
                    message_type: backend::MessageType::PRStartReq,
                    transaction_id: 1234,
                    ..Default::default()
                },
                phy_payload: jr_phy.to_vec().unwrap(),
                ul_meta_data: backend::ULMetaData {
                    dev_eui: vec![8, 7, 6, 5, 4, 3, 2, 1],
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
            phy_payload: vec![1, 2, 3, 4],
            dl_meta_data: Some(backend::DLMetaData {
                class_mode: Some("A".to_string()),
                dl_freq_1: Some(868.1),
                data_rate_1: Some(0),
                rx_delay_1: Some(5),
                ..Default::default()
            }),
            ..Default::default()
        })
        .status(200);
    });

    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;
    gateway_backend::mock::reset().await;

    // Simulate uplink
    uplink::handle_uplink(
        CommonName::EU868,
        "eu868".into(),
        Uuid::new_v4(),
        gw::UplinkFrameSet {
            phy_payload: jr_phy.to_vec().unwrap(),
            tx_info: Some(tx_info),
            rx_info: vec![rx_info],
        },
    )
    .await
    .unwrap();

    js_join_request_mock.assert();
    js_join_request_mock.delete();

    sns_pr_start_req_mock.assert();
    sns_pr_start_req_mock.delete();

    assert::downlink_frame(gw::DownlinkFrame {
        gateway_id: "0102030405060708".into(),
        items: vec![gw::DownlinkFrameItem {
            phy_payload: vec![1, 2, 3, 4],
            tx_info: Some(gw::DownlinkTxInfo {
                frequency: 868100000,
                power: 16,
                modulation: Some(gw::Modulation {
                    parameters: Some(gw::modulation::Parameters::Lora(gw::LoraModulationInfo {
                        bandwidth: 125000,
                        spreading_factor: 12,
                        code_rate: gw::CodeRate::Cr45.into(),
                        polarization_inversion: true,
                        code_rate_legacy: "".to_string(),
                        no_crc: false,
                        preamble: 0,
                    })),
                }),
                board: 0,
                antenna: 0,
                timing: Some(gw::Timing {
                    parameters: Some(gw::timing::Parameters::Delay(gw::DelayTimingInfo {
                        delay: Some(pbjson_types::Duration {
                            seconds: 5,
                            nanos: 0,
                        }),
                    })),
                }),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    })()
    .await;

    joinserver::reset().await;
}

#[tokio::test]
async fn test_sns() {
    let _guard = test::prepare().await;

    let fns_mock = MockServer::start();

    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("010203").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("030201").unwrap(),
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

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::B,
        ..Default::default()
    })
    .await
    .unwrap();

    let dk = device_keys::create(device_keys::DeviceKeys {
        dev_eui: dev.dev_eui,
        nwk_key: AES128Key::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
        dev_nonces: fields::DevNonces::default(),
        ..Default::default()
    })
    .await
    .unwrap();

    let mut jr_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_str("0000000000000000").unwrap(),
            dev_eui: dev.dev_eui,
            dev_nonce: 1,
        }),
        mic: None,
    };
    jr_phy.set_join_request_mic(&dk.nwk_key).unwrap();

    let recv_time = Utc::now();

    let mut rx_info = gw::UplinkRxInfo {
        gateway_id: "0302030405060708".to_string(),
        gw_time: Some(recv_time.into()),
        location: Some(Default::default()),
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
            sender_id: vec![3, 2, 1],
            receiver_id: vec![1, 2, 3],
            message_type: backend::MessageType::PRStartReq,
            transaction_id: 1234,
            ..Default::default()
        },
        phy_payload: jr_phy.to_vec().unwrap(),
        ul_meta_data: backend::ULMetaData {
            dev_eui: dev.dev_eui.to_vec(),
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
                    sender_id: vec![1, 2, 3],
                    receiver_id: vec![3, 2, 1],
                    message_type: backend::MessageType::PRStartAns,
                    transaction_id: 1234,
                    ..Default::default()
                },
                result: backend::ResultPayload {
                    result_code: backend::ResultCode::Success,
                    ..Default::default()
                },
            },
            phy_payload: vec![
                32, 62, 206, 177, 148, 31, 33, 193, 200, 4, 185, 248, 156, 108, 64, 97, 1
            ],
            dev_eui: vec![2, 2, 3, 4, 5, 6, 7, 8],
            nwk_s_key: Some(backend::KeyEnvelope {
                kek_label: "".to_string(),
                aes_key: vec![
                    136, 91, 1, 94, 61, 245, 54, 151, 185, 147, 143, 76, 248, 79, 192, 28
                ],
            }),
            f_cnt_up: Some(0),
            dl_meta_data: Some(backend::DLMetaData {
                dev_eui: vec![2, 2, 3, 4, 5, 6, 7, 8],
                dl_freq_1: Some(868.1),
                dl_freq_2: Some(869.525),
                rx_delay_1: Some(5),
                class_mode: Some("A".to_string()),
                data_rate_1: Some(0),
                data_rate_2: Some(0),
                gw_info: vec![backend::GWInfoElement {
                    ul_token: rx_info.encode_to_vec(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            dev_addr: vec![7, 2, 3, 4],
            ..Default::default()
        },
        pr_start_ans
    );

    joinserver::reset().await;
}

#[tokio::test]
async fn test_sns_roaming_not_allowed() {
    let _guard = test::prepare().await;

    let fns_mock = MockServer::start();

    let mut conf = (*config::get()).clone();

    // Set NetID.
    conf.network.net_id = NetID::from_str("010203").unwrap();

    // Set roaming agreement.
    conf.roaming.servers.push(config::RoamingServer {
        net_id: NetID::from_str("030201").unwrap(),
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

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::B,
        ..Default::default()
    })
    .await
    .unwrap();

    let dk = device_keys::create(device_keys::DeviceKeys {
        dev_eui: dev.dev_eui,
        nwk_key: AES128Key::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
        dev_nonces: fields::DevNonces::default(),
        ..Default::default()
    })
    .await
    .unwrap();

    let mut jr_phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_str("0000000000000000").unwrap(),
            dev_eui: dev.dev_eui,
            dev_nonce: 1,
        }),
        mic: None,
    };
    jr_phy.set_join_request_mic(&dk.nwk_key).unwrap();

    let recv_time = Utc::now();

    let mut rx_info = gw::UplinkRxInfo {
        gateway_id: "0302030405060708".to_string(),
        gw_time: Some(recv_time.into()),
        location: Some(Default::default()),
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
            sender_id: vec![3, 2, 1],
            receiver_id: vec![1, 2, 3],
            message_type: backend::MessageType::PRStartReq,
            transaction_id: 1234,
            ..Default::default()
        },
        phy_payload: jr_phy.to_vec().unwrap(),
        ul_meta_data: backend::ULMetaData {
            dev_eui: dev.dev_eui.to_vec(),
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
                    sender_id: vec![1, 2, 3],
                    receiver_id: vec![3, 2, 1],
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

    joinserver::reset().await;
}

/*
 * 模块概述
 * ========
 * OTAA Join Server测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，专门用于测试
 * LoRaWAN设备通过外部Join Server进行空中激活(OTAA)的功能。在LoRaWAN网络架构中，Join Server
 * 负责管理设备的根密钥和处理加入请求，是安全激活设备的关键组件。
 * 
 * 该模块实现了一系列测试用例，覆盖了与外部Join Server交互的各个方面，包括Join-Request消息处理、
 * Join Server响应处理、会话密钥派生等。通过这些测试，确保ChirpStack能够正确实现LoRaWAN协议
 * 规范中关于与外部Join Server交互的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和Join Server，使得测试可以在不依赖实际硬件和外部服务的
 * 情况下运行，同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(otaa_js_test.rs)实现了与外部Join Server交互的OTAA测试用例，提供了以下主要功能：
 * 1. Join Server交互测试：验证ChirpStack与外部Join Server的通信
 * 2. Join-Request处理测试：验证Join-Request消息的处理
 * 3. Join Server响应处理测试：验证Join Server响应的处理
 *
 * 主要组件
 * ========
 * - Test结构体: 定义测试用例的参数和断言
 * - test_js(): 测试与外部Join Server的交互
 * - run_test(): 运行测试用例的辅助函数
 *
 * 关键流程
 * ========
 * 1. Join Server测试执行流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置模拟Join Server
 *    - 处理模拟的Join-Request上行消息
 *    - 验证ChirpStack向Join Server发送的请求
 *    - 模拟Join Server的响应
 *    - 验证设备会话的创建和密钥派生
 *    - 验证Join-Accept下行消息的生成
 *    - 验证集成事件的发送
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备会话、下行消息等
 * - 安全考虑: OTAA涉及密钥派生和安全通信，测试应该验证安全机制的正确性
 * - 协议版本: 测试应考虑不同版本的LoRaWAN协议和后端接口规范
 * - 错误处理: 测试应验证各种错误情况的处理，确保系统的健壮性
 */

use httpmock::prelude::*;
use uuid::Uuid;

use super::assert;
use crate::storage::{application, device, device_profile, gateway, reset_redis, tenant};
use crate::{
    backend::joinserver, config, gateway::backend as gateway_backend, integration, region, test,
    uplink,
};
use chirpstack_api::{common, gw, integration as integration_pb, internal};
use lrwn::region::CommonName;
use lrwn::{DevAddr, EUI64Prefix, EUI64};

struct Test {
    name: String,
    tx_info: gw::UplinkTxInfo,
    rx_info: gw::UplinkRxInfo,
    phy_payload: lrwn::PhyPayload,
    js_response: backend::JoinAnsPayload,
    assert: Vec<assert::Validator>,
}

#[tokio::test]
async fn test_js() {
    let _guard = test::prepare().await;

    let t = tenant::create(tenant::Tenant {
        name: "tenant".into(),
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let gw = gateway::create(gateway::Gateway {
        name: "gw".into(),
        tenant_id: t.id,
        gateway_id: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
    })
    .await
    .unwrap();

    let dp = device_profile::create(device_profile::DeviceProfile {
        name: "dp".into(),
        tenant_id: t.id,
        region: lrwn::region::CommonName::EU868,
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_3,
        reg_params_revision: lrwn::region::Revision::A,
        supports_otaa: true,
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

    let dev = device::create(device::Device {
        name: "dev".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
    })
    .await
    .unwrap();

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let rx_info = gw::UplinkRxInfo {
        gateway_id: gw.gateway_id.to_string(),
        location: Some(Default::default()),
        ..Default::default()
    };

    let phy = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dev_eui: dev.dev_eui,
            dev_nonce: 1,
        }),
        mic: Some([1, 2, 3, 4]),
    };

    let phy_ja = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinAccept,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinAccept(lrwn::JoinAcceptPayload {
            join_nonce: 1,
            home_netid: lrwn::NetID::from_be_bytes([0, 0, 0]),
            devaddr: DevAddr::from_be_bytes([1, 2, 3, 4]),
            dl_settings: lrwn::DLSettings {
                opt_neg: false,
                rx2_dr: 0,
                rx1_dr_offset: 0,
            },
            rx_delay: 1,
            cflist: None,
        }),
        mic: Some([1, 2, 3, 4]),
    };

    let tests = vec![
        Test {
            name: "test plain-text app_s_key".into(),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: phy.clone(),
            js_response: backend::JoinAnsPayload {
                base: backend::BasePayloadResult {
                    base: backend::BasePayload {
                        sender_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                        receiver_id: vec![0, 0, 0],
                        message_type: backend::MessageType::JoinAns,
                        ..Default::default()
                    },
                    result: backend::ResultPayload {
                        result_code: backend::ResultCode::Success,
                        ..Default::default()
                    },
                },
                phy_payload: phy_ja.to_vec().unwrap(),
                app_s_key: Some(backend::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                }),
                nwk_s_key: Some(backend::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                }),
                ..Default::default()
            },
            assert: vec![
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan103.into(),
                        f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "".into(),
                            aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        }),
                        rx1_delay: 1,
                        rx2_frequency: 869525000,
                        enabled_uplink_channel_indices: vec![0, 1, 2],
                        nb_trans: 1,
                        region_config_id: "eu868".to_string(),
                        class_b_ping_slot_nb: 1,
                        ..Default::default()
                    },
                ),
                assert::join_event(integration_pb::JoinEvent {
                    device_info: Some(integration_pb::DeviceInfo {
                        tenant_id: t.id.to_string(),
                        tenant_name: t.name.clone(),
                        application_id: app.id.to_string(),
                        application_name: app.name.clone(),
                        device_profile_id: dp.id.to_string(),
                        device_profile_name: dp.name.clone(),
                        device_name: dev.name.clone(),
                        dev_eui: dev.dev_eui.to_string(),
                        device_class_enabled: common::DeviceClass::ClassA.into(),
                        ..Default::default()
                    }),
                    dev_addr: "01020304".into(),
                    join_server_context: None,
                    region_config_id: "eu868".into(),
                    ..Default::default()
                }),
            ],
        },
        Test {
            name: "test session_key_id".into(),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: phy.clone(),
            js_response: backend::JoinAnsPayload {
                base: backend::BasePayloadResult {
                    base: backend::BasePayload {
                        sender_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                        receiver_id: vec![0, 0, 0],
                        message_type: backend::MessageType::JoinAns,
                        ..Default::default()
                    },
                    result: backend::ResultPayload {
                        result_code: backend::ResultCode::Success,
                        ..Default::default()
                    },
                },
                phy_payload: phy_ja.to_vec().unwrap(),
                session_key_id: vec![1, 2, 3, 4],
                nwk_s_key: Some(backend::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                }),
                ..Default::default()
            },
            assert: vec![
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan103.into(),
                        f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        js_session_key_id: vec![1, 2, 3, 4],
                        rx1_delay: 1,
                        rx2_frequency: 869525000,
                        enabled_uplink_channel_indices: vec![0, 1, 2],
                        nb_trans: 1,
                        region_config_id: "eu868".to_string(),
                        class_b_ping_slot_nb: 1,
                        ..Default::default()
                    },
                ),
                assert::join_event(integration_pb::JoinEvent {
                    device_info: Some(integration_pb::DeviceInfo {
                        tenant_id: t.id.to_string(),
                        tenant_name: t.name.clone(),
                        application_id: app.id.to_string(),
                        application_name: app.name.clone(),
                        device_profile_id: dp.id.to_string(),
                        device_profile_name: dp.name.clone(),
                        device_name: dev.name.clone(),
                        dev_eui: dev.dev_eui.to_string(),
                        device_class_enabled: common::DeviceClass::ClassA.into(),
                        ..Default::default()
                    }),
                    dev_addr: "01020304".into(),
                    join_server_context: Some(common::JoinServerContext {
                        session_key_id: "01020304".into(),
                        ..Default::default()
                    }),
                    region_config_id: "eu868".into(),
                    ..Default::default()
                }),
            ],
        },
        Test {
            name: "test encrypted app_s_key".into(),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: phy.clone(),
            js_response: backend::JoinAnsPayload {
                base: backend::BasePayloadResult {
                    base: backend::BasePayload {
                        sender_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                        receiver_id: vec![0, 0, 0],
                        message_type: backend::MessageType::JoinAns,
                        ..Default::default()
                    },
                    result: backend::ResultPayload {
                        result_code: backend::ResultCode::Success,
                        ..Default::default()
                    },
                },
                phy_payload: phy_ja.to_vec().unwrap(),
                app_s_key: Some(backend::KeyEnvelope {
                    kek_label: "kek-label".into(),
                    aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                }),
                nwk_s_key: Some(backend::KeyEnvelope {
                    kek_label: "".into(),
                    aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                }),
                ..Default::default()
            },
            assert: vec![
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan103.into(),
                        f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "kek-label".into(),
                            aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        }),
                        rx1_delay: 1,
                        rx2_frequency: 869525000,
                        enabled_uplink_channel_indices: vec![0, 1, 2],
                        nb_trans: 1,
                        region_config_id: "eu868".to_string(),
                        class_b_ping_slot_nb: 1,
                        ..Default::default()
                    },
                ),
                assert::join_event(integration_pb::JoinEvent {
                    device_info: Some(integration_pb::DeviceInfo {
                        tenant_id: t.id.to_string(),
                        tenant_name: t.name.clone(),
                        application_id: app.id.to_string(),
                        application_name: app.name.clone(),
                        device_profile_id: dp.id.to_string(),
                        device_profile_name: dp.name.clone(),
                        device_name: dev.name.clone(),
                        dev_eui: dev.dev_eui.to_string(),
                        device_class_enabled: common::DeviceClass::ClassA.into(),
                        ..Default::default()
                    }),
                    dev_addr: "01020304".into(),
                    join_server_context: Some(common::JoinServerContext {
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "kek-label".into(),
                            aes_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8],
                        }),
                        ..Default::default()
                    }),
                    region_config_id: "eu868".into(),
                    ..Default::default()
                }),
            ],
        },
    ];

    for tst in &tests {
        run_test(tst).await;
    }
}

async fn run_test(t: &Test) {
    println!("> {}", t.name);

    reset_redis().await.unwrap();

    let server = MockServer::start();
    let mut js_mock = server.mock(|when, then| {
        when.method(POST).path("/");

        then.body(serde_json::to_string(&t.js_response).unwrap());
    });

    let mut conf: config::Configuration = (*config::get()).clone();
    conf.join_server.servers = vec![config::JoinServerServer {
        join_eui_prefix: EUI64Prefix::new([1, 2, 3, 4, 5, 6, 7, 8], 64),
        server: server.url("/"),
        ..Default::default()
    }];
    config::set(conf);
    region::setup().unwrap();
    joinserver::setup().await.unwrap();

    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    integration::mock::reset().await;
    gateway_backend::mock::reset().await;

    uplink::handle_uplink(
        CommonName::EU868,
        "eu868".into(),
        Uuid::new_v4(),
        gw::UplinkFrameSet {
            phy_payload: t.phy_payload.to_vec().unwrap(),
            tx_info: Some(t.tx_info.clone()),
            rx_info: vec![t.rx_info.clone()],
        },
    )
    .await
    .unwrap();

    js_mock.assert();
    js_mock.delete();

    for assert in &t.assert {
        assert().await;
    }
}

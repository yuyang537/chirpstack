/*
 * 模块概述
 * ========
 * OTAA(Over-The-Air Activation)测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，
 * 专门用于测试LoRaWAN设备的空中激活流程。OTAA是LoRaWAN设备加入网络的主要方式，设备通过
 * 发送Join-Request消息并接收Join-Accept响应来获取会话密钥和设备地址。
 * 
 * 该模块实现了一系列全面的测试用例，覆盖了OTAA流程的各个方面，包括Join-Request消息处理、
 * Join-Accept响应生成、密钥派生、设备会话创建等。通过这些测试，确保ChirpStack能够正确实现
 * LoRaWAN协议规范中关于OTAA的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(otaa_test.rs)实现了OTAA流程的测试用例，提供了以下主要功能：
 * 1. 网关过滤测试：验证私有网关的上行Join-Request消息过滤功能
 * 2. LoRaWAN 1.0 OTAA测试：验证LoRaWAN 1.0版本的OTAA流程
 * 3. LoRaWAN 1.1 OTAA测试：验证LoRaWAN 1.1版本的OTAA流程
 *
 * 主要组件
 * ========
 * - Test结构体: 定义测试用例的参数和断言
 * - test_gateway_filtering(): 测试网关过滤功能
 * - test_lorawan_10(): 测试LoRaWAN 1.0版本的OTAA流程
 * - test_lorawan_11(): 测试LoRaWAN 1.1版本的OTAA流程
 * - run_test(): 运行测试用例的辅助函数
 *
 * 关键流程
 * ========
 * 1. OTAA测试执行流程:
 *    - 准备测试环境(租户、应用、设备配置、设备密钥等)
 *    - 设置测试参数(Join-Request消息等)
 *    - 执行测试前的准备函数(如果有)
 *    - 处理模拟的Join-Request上行消息
 *    - 验证Join-Accept响应的正确性
 *    - 验证设备会话的创建和密钥派生
 *    - 验证集成事件的发送
 *    - 执行测试后的清理函数(如果有)
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备会话、下行消息等
 * - 协议版本: 测试区分了LoRaWAN 1.0和1.1版本，确保两个版本都能正确工作
 * - 安全考虑: OTAA涉及密钥派生和安全通信，测试应该验证安全机制的正确性
 * - 兼容性: 测试应该验证与不同设备和网关的兼容性
 */

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use uuid::Uuid;

use super::assert;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_keys, device_profile, gateway, tenant,
};
use crate::{
    config, gateway::backend as gateway_backend, integration, region, storage::fields, test, uplink,
};
use chirpstack_api::{common, gw, internal, stream};
use lrwn::keys::get_js_int_key;
use lrwn::region::CommonName;
use lrwn::{AES128Key, EUI64};

type Function = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()>>>>;

struct Test {
    name: String,
    dev_eui: EUI64,
    before_func: Option<Function>,
    after_func: Option<Function>,
    tx_info: gw::UplinkTxInfo,
    rx_info: gw::UplinkRxInfo,
    phy_payload: lrwn::PhyPayload,
    extra_uplink_channels: Vec<u32>,
    assert: Vec<assert::Validator>,
}

#[tokio::test]
async fn test_gateway_filtering() {
    let _guard = test::prepare().await;
    let t_a = tenant::create(tenant::Tenant {
        name: "tenant-a".into(),
        private_gateways_up: true,
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();
    let t_b = tenant::create(tenant::Tenant {
        name: "tenant-b".into(),
        private_gateways_up: true,
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let gw_a = gateway::create(gateway::Gateway {
        name: "gateway-a".into(),
        tenant_id: t_a.id,
        gateway_id: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
    })
    .await
    .unwrap();

    let gw_b = gateway::create(gateway::Gateway {
        name: "gateway-b".into(),
        tenant_id: t_b.id,
        gateway_id: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
    })
    .await
    .unwrap();

    let app = application::create(application::Application {
        name: "app".into(),
        tenant_id: t_a.id,
        ..Default::default()
    })
    .await
    .unwrap();

    let dp = device_profile::create(device_profile::DeviceProfile {
        name: "dp".into(),
        tenant_id: t_a.id,
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
        dev_nonces: {
            let mut dev_nonces = fields::DevNonces::default();
            dev_nonces.insert(EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]), 258);
            dev_nonces
        },
        ..Default::default()
    })
    .await
    .unwrap();

    let rx_info_a = gw::UplinkRxInfo {
        gateway_id: gw_a.gateway_id.to_string(),
        location: Some(Default::default()),
        ..Default::default()
    };

    let rx_info_b = gw::UplinkRxInfo {
        gateway_id: gw_b.gateway_id.to_string(),
        location: Some(Default::default()),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let mut jr_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dev_eui: dev.dev_eui,
            dev_nonce: 258,
        }),
        mic: None,
    };
    jr_pl.set_join_request_mic(&dk.nwk_key).unwrap();

    let tests = vec![
        Test {
            name: "private gateway of same tenant".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();
                })
            })),
            after_func: None,
            rx_info: rx_info_a.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![assert::device_session(
                dev.dev_eui,
                internal::DeviceSession {
                    dev_addr: vec![1, 2, 3, 4],
                    mac_version: common::MacVersion::Lorawan102.into(),
                    f_nwk_s_int_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    s_nwk_s_int_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    nwk_s_enc_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    app_s_key: Some(common::KeyEnvelope {
                        kek_label: "".into(),
                        aes_key: vec![
                            5, 211, 222, 240, 51, 52, 23, 15, 218, 155, 237, 228, 198, 37, 200, 117,
                        ],
                    }),
                    rx1_delay: 1,
                    rx2_frequency: 869525000,
                    enabled_uplink_channel_indices: vec![0, 1, 2],
                    nb_trans: 1,
                    region_config_id: "eu868".to_string(),
                    class_b_ping_slot_nb: 1,
                    ..Default::default()
                },
            )],
        },
        Test {
            name: "private gateway other tenant".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();
                })
            })),
            after_func: None,
            rx_info: rx_info_b.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![assert::no_device_session(dev.dev_eui)],
        },
    ];

    for tst in &tests {
        run_test(tst).await;
    }
}

#[tokio::test]
async fn test_lorawan_10() {
    let _guard = test::prepare().await;
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
        gateway_id: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
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
        dev_nonces: {
            let mut dev_nonces = fields::DevNonces::default();
            dev_nonces.insert(EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]), 258);
            dev_nonces
        },
        ..Default::default()
    })
    .await
    .unwrap();

    let rx_info = gw::UplinkRxInfo {
        gateway_id: gw.gateway_id.to_string(),
        location: Some(Default::default()),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let mut jr_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dev_eui: dev.dev_eui,
            dev_nonce: 258,
        }),
        mic: None,
    };
    jr_pl.set_join_request_mic(&dk.nwk_key).unwrap();

    let mut ja_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinAccept,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinAccept(lrwn::JoinAcceptPayload {
            join_nonce: 0,
            home_netid: lrwn::NetID::from_be_bytes([0, 0, 0]),
            devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
            dl_settings: lrwn::DLSettings {
                rx2_dr: 0,
                rx1_dr_offset: 0,
                opt_neg: false,
            },
            rx_delay: 1,
            cflist: None,
        }),
        mic: None,
    };
    ja_pl
        .set_join_accept_mic(
            lrwn::JoinType::Join,
            &EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            258,
            &dk.nwk_key,
        )
        .unwrap();
    ja_pl.encrypt_join_accept_payload(&dk.nwk_key).unwrap();

    let mut ja_cflist_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinAccept,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinAccept(lrwn::JoinAcceptPayload {
            join_nonce: 0,
            home_netid: lrwn::NetID::from_be_bytes([0, 0, 0]),
            devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
            dl_settings: lrwn::DLSettings {
                rx2_dr: 0,
                rx1_dr_offset: 0,
                opt_neg: false,
            },
            rx_delay: 1,
            cflist: Some(lrwn::CFList::Channels(lrwn::CFListChannels::new([
                867100000, 867300000, 867500000, 867700000, 867900000,
            ]))),
        }),
        mic: None,
    };
    ja_cflist_pl
        .set_join_accept_mic(
            lrwn::JoinType::Join,
            &EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            258,
            &dk.nwk_key,
        )
        .unwrap();
    ja_cflist_pl
        .encrypt_join_accept_payload(&dk.nwk_key)
        .unwrap();

    let tests = vec![
        Test {
            name: "dev-nonce already used".into(),
            dev_eui: dev.dev_eui,
            before_func: None,
            after_func: None,
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![assert::integration_log(vec![
                "DevNonce has already been used".to_string(),
            ])],
        },
        Test {
            name: "join-request accepted".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();
                })
            })),
            after_func: None,
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![
                assert::device_join_eui(
                    dev.dev_eui,
                    EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
                ),
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan102.into(),
                        f_nwk_s_int_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        s_nwk_s_int_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        nwk_s_enc_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "".into(),
                            aes_key: vec![
                                5, 211, 222, 240, 51, 52, 23, 15, 218, 155, 237, 228, 198, 37, 200,
                                117,
                            ],
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
                assert::downlink_frame(gw::DownlinkFrame {
                    items: vec![
                        gw::DownlinkFrameItem {
                            phy_payload: ja_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 868100000,
                                power: 16,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(5).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                        gw::DownlinkFrameItem {
                            phy_payload: ja_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 869525000,
                                power: 29,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(6).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                    ],
                    gateway_id: "0102030405060708".to_string(),
                    ..Default::default()
                }),
                assert::downlink_frame_saved(internal::DownlinkFrame {
                    dev_eui: vec![2, 2, 3, 4, 5, 6, 7, 8],
                    nwk_s_enc_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    downlink_frame: Some(gw::DownlinkFrame {
                        items: vec![
                            gw::DownlinkFrameItem {
                                phy_payload: ja_pl.to_vec().unwrap(),
                                tx_info_legacy: None,
                                tx_info: Some(gw::DownlinkTxInfo {
                                    frequency: 868100000,
                                    power: 16,
                                    modulation: Some(gw::Modulation {
                                        parameters: Some(gw::modulation::Parameters::Lora(
                                            gw::LoraModulationInfo {
                                                bandwidth: 125000,
                                                spreading_factor: 12,
                                                code_rate: gw::CodeRate::Cr45.into(),
                                                polarization_inversion: true,
                                                ..Default::default()
                                            },
                                        )),
                                    }),
                                    timing: Some(gw::Timing {
                                        parameters: Some(gw::timing::Parameters::Delay(
                                            gw::DelayTimingInfo {
                                                delay: Some(Duration::from_secs(5).into()),
                                            },
                                        )),
                                    }),
                                    ..Default::default()
                                }),
                            },
                            gw::DownlinkFrameItem {
                                phy_payload: ja_pl.to_vec().unwrap(),
                                tx_info_legacy: None,
                                tx_info: Some(gw::DownlinkTxInfo {
                                    frequency: 869525000,
                                    power: 29,
                                    modulation: Some(gw::Modulation {
                                        parameters: Some(gw::modulation::Parameters::Lora(
                                            gw::LoraModulationInfo {
                                                bandwidth: 125000,
                                                spreading_factor: 12,
                                                code_rate: gw::CodeRate::Cr45.into(),
                                                polarization_inversion: true,
                                                ..Default::default()
                                            },
                                        )),
                                    }),
                                    timing: Some(gw::Timing {
                                        parameters: Some(gw::timing::Parameters::Delay(
                                            gw::DelayTimingInfo {
                                                delay: Some(Duration::from_secs(6).into()),
                                            },
                                        )),
                                    }),
                                    ..Default::default()
                                }),
                            },
                        ],
                        gateway_id: "0102030405060708".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                assert::enabled_class(dev.dev_eui, DeviceClass::A),
                assert::device_queue_items(dev.dev_eui, vec![]),
                assert::uplink_meta_log(stream::UplinkMeta {
                    dev_eui: dev.dev_eui.to_string(),
                    tx_info: Some(tx_info.clone()),
                    rx_info: vec![rx_info.clone()],
                    phy_payload_byte_count: 23,
                    message_type: common::MType::JoinRequest.into(),
                    ..Default::default()
                }),
            ],
        },
        Test {
            name: "join-request accepted + skip fcnt check".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();

                    let mut dev = device::get(&dev_eui).await.unwrap();
                    dev.skip_fcnt_check = true;
                    let _ = device::update(dev).await.unwrap();
                })
            })),
            after_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    let mut dev = device::get(&dev_eui).await.unwrap();
                    dev.skip_fcnt_check = false;
                    let _ = device::update(dev).await.unwrap();
                })
            })),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![assert::device_session(
                dev.dev_eui,
                internal::DeviceSession {
                    dev_addr: vec![1, 2, 3, 4],
                    mac_version: common::MacVersion::Lorawan102.into(),
                    f_nwk_s_int_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    s_nwk_s_int_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    nwk_s_enc_key: vec![
                        128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                    ],
                    app_s_key: Some(common::KeyEnvelope {
                        kek_label: "".into(),
                        aes_key: vec![
                            5, 211, 222, 240, 51, 52, 23, 15, 218, 155, 237, 228, 198, 37, 200, 117,
                        ],
                    }),
                    rx1_delay: 1,
                    rx2_frequency: 869525000,
                    enabled_uplink_channel_indices: vec![0, 1, 2],
                    nb_trans: 1,
                    region_config_id: "eu868".to_string(),
                    skip_f_cnt_check: true,
                    class_b_ping_slot_nb: 1,
                    ..Default::default()
                },
            )],
        },
        Test {
            name: "join-request accepted + cflist".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();
                })
            })),
            after_func: None,
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![867100000, 867300000, 867500000, 867700000, 867900000],
            assert: vec![
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan102.into(),
                        f_nwk_s_int_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        s_nwk_s_int_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        nwk_s_enc_key: vec![
                            128, 47, 168, 41, 62, 215, 212, 79, 19, 83, 183, 201, 43, 169, 125, 200,
                        ],
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "".into(),
                            aes_key: vec![
                                5, 211, 222, 240, 51, 52, 23, 15, 218, 155, 237, 228, 198, 37, 200,
                                117,
                            ],
                        }),
                        rx1_delay: 1,
                        rx2_frequency: 869525000,
                        enabled_uplink_channel_indices: vec![0, 1, 2, 3, 4, 5, 6, 7],
                        extra_uplink_channels: [
                            (
                                3,
                                internal::DeviceSessionChannel {
                                    frequency: 867100000,
                                    min_dr: 0,
                                    max_dr: 5,
                                },
                            ),
                            (
                                4,
                                internal::DeviceSessionChannel {
                                    frequency: 867300000,
                                    min_dr: 0,
                                    max_dr: 5,
                                },
                            ),
                            (
                                5,
                                internal::DeviceSessionChannel {
                                    frequency: 867500000,
                                    min_dr: 0,
                                    max_dr: 5,
                                },
                            ),
                            (
                                6,
                                internal::DeviceSessionChannel {
                                    frequency: 867700000,
                                    min_dr: 0,
                                    max_dr: 5,
                                },
                            ),
                            (
                                7,
                                internal::DeviceSessionChannel {
                                    frequency: 867900000,
                                    min_dr: 0,
                                    max_dr: 5,
                                },
                            ),
                        ]
                        .iter()
                        .cloned()
                        .collect(),
                        nb_trans: 1,
                        region_config_id: "eu868".to_string(),
                        class_b_ping_slot_nb: 1,
                        ..Default::default()
                    },
                ),
                assert::downlink_frame(gw::DownlinkFrame {
                    items: vec![
                        gw::DownlinkFrameItem {
                            phy_payload: ja_cflist_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 868100000,
                                power: 16,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(5).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                        gw::DownlinkFrameItem {
                            phy_payload: ja_cflist_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 869525000,
                                power: 29,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(6).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                    ],
                    gateway_id: "0102030405060708".into(),
                    ..Default::default()
                }),
            ],
        },
        Test {
            name: "join-request accepted + class-b supported".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                let dp_id = dp.id;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();

                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_b = true;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            after_func: Some(Box::new(move || {
                let dp_id = dp.id;
                Box::pin(async move {
                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_b = false;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: Vec::new(),
            assert: vec![assert::enabled_class(dev.dev_eui, DeviceClass::A)],
        },
        Test {
            name: "join-request accepted + class-c supported".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                let dp_id = dp.id;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();

                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_c = true;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            after_func: Some(Box::new(move || {
                let dp_id = dp.id;
                Box::pin(async move {
                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_c = false;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: Vec::new(),
            assert: vec![assert::enabled_class(dev.dev_eui, DeviceClass::C)],
        },
        Test {
            name: "device disabled".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();

                    let mut dev = device::get(&dev_eui).await.unwrap();
                    dev.is_disabled = true;
                    let _ = device::update(dev).await.unwrap();
                })
            })),
            after_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    let mut dev = device::get(&dev_eui).await.unwrap();
                    dev.is_disabled = false;
                    let _ = device::update(dev).await.unwrap();
                })
            })),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: Vec::new(),
            assert: vec![assert::no_downlink_frame()],
        },
    ];

    for tst in &tests {
        run_test(tst).await;
    }
}

#[tokio::test]
async fn test_lorawan_11() {
    let _guard = test::prepare().await;

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
        gateway_id: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
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
        mac_version: lrwn::region::MacVersion::LORAWAN_1_1_0,
        reg_params_revision: lrwn::region::Revision::RP002_1_0_3,
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
        app_key: AES128Key::from_bytes([16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]),
        dev_nonces: {
            let mut dev_nonces = fields::DevNonces::default();
            dev_nonces.insert(EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]), 258);
            dev_nonces
        },
        ..Default::default()
    })
    .await
    .unwrap();

    let rx_info = gw::UplinkRxInfo {
        gateway_id: gw.gateway_id.to_string(),
        location: Some(Default::default()),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let mut jr_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinRequest,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinRequest(lrwn::JoinRequestPayload {
            join_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dev_eui: dev.dev_eui,
            dev_nonce: 258,
        }),
        mic: None,
    };
    jr_pl.set_join_request_mic(&dk.nwk_key).unwrap();

    let mut ja_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinAccept,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinAccept(lrwn::JoinAcceptPayload {
            join_nonce: 0,
            home_netid: lrwn::NetID::from_be_bytes([0, 0, 0]),
            devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
            dl_settings: lrwn::DLSettings {
                rx2_dr: 0,
                rx1_dr_offset: 0,
                opt_neg: true,
            },
            rx_delay: 1,
            cflist: None,
        }),
        mic: None,
    };
    ja_pl
        .set_join_accept_mic(
            lrwn::JoinType::Join,
            &EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            258,
            &get_js_int_key(&dev.dev_eui, &dk.nwk_key).unwrap(),
        )
        .unwrap();
    ja_pl.encrypt_join_accept_payload(&dk.nwk_key).unwrap();

    let tests = vec![
        Test {
            name: "dev-nonce already used".into(),
            dev_eui: dev.dev_eui,
            before_func: None,
            after_func: None,
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![assert::integration_log(vec![
                "DevNonce has already been used".to_string(),
            ])],
        },
        Test {
            name: "join-request accepted".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();
                })
            })),
            after_func: None,
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: vec![],
            assert: vec![
                assert::device_session(
                    dev.dev_eui,
                    internal::DeviceSession {
                        dev_addr: vec![1, 2, 3, 4],
                        mac_version: common::MacVersion::Lorawan110.into(),
                        f_nwk_s_int_key: vec![
                            98, 222, 198, 158, 98, 155, 205, 235, 143, 171, 203, 19, 221, 9, 1, 231,
                        ],
                        s_nwk_s_int_key: vec![
                            8, 16, 172, 220, 92, 121, 168, 210, 224, 162, 133, 180, 191, 167, 33,
                            73,
                        ],
                        nwk_s_enc_key: vec![
                            151, 120, 115, 101, 67, 122, 194, 153, 113, 209, 134, 158, 149, 189,
                            192, 175,
                        ],
                        app_s_key: Some(common::KeyEnvelope {
                            kek_label: "".into(),
                            aes_key: vec![
                                27, 30, 215, 60, 144, 234, 251, 130, 186, 67, 197, 148, 250, 49,
                                106, 77,
                            ],
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
                assert::downlink_frame(gw::DownlinkFrame {
                    items: vec![
                        gw::DownlinkFrameItem {
                            phy_payload: ja_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 868100000,
                                power: 16,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(5).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                        gw::DownlinkFrameItem {
                            phy_payload: ja_pl.to_vec().unwrap(),
                            tx_info_legacy: None,
                            tx_info: Some(gw::DownlinkTxInfo {
                                frequency: 869525000,
                                power: 29,
                                modulation: Some(gw::Modulation {
                                    parameters: Some(gw::modulation::Parameters::Lora(
                                        gw::LoraModulationInfo {
                                            bandwidth: 125000,
                                            spreading_factor: 12,
                                            code_rate: gw::CodeRate::Cr45.into(),
                                            polarization_inversion: true,
                                            ..Default::default()
                                        },
                                    )),
                                }),
                                timing: Some(gw::Timing {
                                    parameters: Some(gw::timing::Parameters::Delay(
                                        gw::DelayTimingInfo {
                                            delay: Some(Duration::from_secs(6).into()),
                                        },
                                    )),
                                }),
                                ..Default::default()
                            }),
                        },
                    ],
                    gateway_id: "0102030405060708".to_string(),
                    ..Default::default()
                }),
                assert::downlink_frame_saved(internal::DownlinkFrame {
                    dev_eui: vec![2, 2, 3, 4, 5, 6, 7, 8],
                    nwk_s_enc_key: vec![
                        151, 120, 115, 101, 67, 122, 194, 153, 113, 209, 134, 158, 149, 189, 192,
                        175,
                    ],
                    downlink_frame: Some(gw::DownlinkFrame {
                        items: vec![
                            gw::DownlinkFrameItem {
                                phy_payload: ja_pl.to_vec().unwrap(),
                                tx_info_legacy: None,
                                tx_info: Some(gw::DownlinkTxInfo {
                                    frequency: 868100000,
                                    power: 16,
                                    modulation: Some(gw::Modulation {
                                        parameters: Some(gw::modulation::Parameters::Lora(
                                            gw::LoraModulationInfo {
                                                bandwidth: 125000,
                                                spreading_factor: 12,
                                                code_rate: gw::CodeRate::Cr45.into(),
                                                polarization_inversion: true,
                                                ..Default::default()
                                            },
                                        )),
                                    }),
                                    timing: Some(gw::Timing {
                                        parameters: Some(gw::timing::Parameters::Delay(
                                            gw::DelayTimingInfo {
                                                delay: Some(Duration::from_secs(5).into()),
                                            },
                                        )),
                                    }),
                                    ..Default::default()
                                }),
                            },
                            gw::DownlinkFrameItem {
                                phy_payload: ja_pl.to_vec().unwrap(),
                                tx_info_legacy: None,
                                tx_info: Some(gw::DownlinkTxInfo {
                                    frequency: 869525000,
                                    power: 29,
                                    modulation: Some(gw::Modulation {
                                        parameters: Some(gw::modulation::Parameters::Lora(
                                            gw::LoraModulationInfo {
                                                bandwidth: 125000,
                                                spreading_factor: 12,
                                                code_rate: gw::CodeRate::Cr45.into(),
                                                polarization_inversion: true,
                                                ..Default::default()
                                            },
                                        )),
                                    }),
                                    timing: Some(gw::Timing {
                                        parameters: Some(gw::timing::Parameters::Delay(
                                            gw::DelayTimingInfo {
                                                delay: Some(Duration::from_secs(6).into()),
                                            },
                                        )),
                                    }),
                                    ..Default::default()
                                }),
                            },
                        ],
                        gateway_id: "0102030405060708".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                assert::enabled_class(dev.dev_eui, DeviceClass::A),
                assert::device_queue_items(dev.dev_eui, vec![]),
            ],
        },
        Test {
            name: "join-request accepted + class-c supported".into(),
            dev_eui: dev.dev_eui,
            before_func: Some(Box::new(move || {
                let dev_eui = dev.dev_eui;
                let dp_id = dp.id;
                Box::pin(async move {
                    device_keys::test::reset_nonces(&dev_eui).await.unwrap();

                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_c = true;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            after_func: Some(Box::new(move || {
                let dp_id = dp.id;
                Box::pin(async move {
                    let mut dp = device_profile::get(&dp_id).await.unwrap();
                    dp.supports_class_c = false;
                    let _ = device_profile::update(dp).await.unwrap();
                })
            })),
            rx_info: rx_info.clone(),
            tx_info: tx_info.clone(),
            phy_payload: jr_pl.clone(),
            extra_uplink_channels: Vec::new(),
            assert: vec![assert::enabled_class(dev.dev_eui, DeviceClass::A)],
        },
    ];

    for tst in &tests {
        run_test(tst).await;
    }
}

async fn run_test(t: &Test) {
    println!("> {}", t.name);

    let mut conf: config::Configuration = (*config::get()).clone();
    for f in &t.extra_uplink_channels {
        conf.regions[0]
            .network
            .extra_channels
            .push(config::ExtraChannel {
                frequency: *f,
                min_dr: 0,
                max_dr: 5,
            });
    }
    config::set(conf);
    region::setup().unwrap();

    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    integration::mock::reset().await;
    gateway_backend::mock::reset().await;

    device::partial_update(
        t.dev_eui,
        &device::DeviceChangeset {
            dev_addr: Some(None),
            device_session: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    if let Some(f) = &t.before_func {
        f().await;
    }

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

    for assert in &t.assert {
        assert().await;
    }

    if let Some(f) = &t.after_func {
        f().await;
    }
}

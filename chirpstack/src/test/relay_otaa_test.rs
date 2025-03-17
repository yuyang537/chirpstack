/*
 * 模块概述
 * ========
 * 中继OTAA(Relay Over-The-Air Activation)测试模块是ChirpStack LoRaWAN网络服务器测试框架的
 * 重要组成部分，专门用于测试LoRaWAN中继设备的空中激活流程。LoRaWAN中继功能是LoRaWAN 1.0.4
 * 规范中引入的新特性，允许通过中继设备扩展网络覆盖范围，使得终端设备可以通过中继设备与网关通信。
 * 
 * 该模块实现了一系列测试用例，覆盖了中继设备OTAA流程的各个方面，包括中继设备的Join-Request
 * 消息处理、Join-Accept响应生成、密钥派生、设备会话创建等。通过这些测试，确保ChirpStack能够
 * 正确实现LoRaWAN协议规范中关于中继设备OTAA的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(relay_otaa_test.rs)实现了中继设备OTAA流程的测试用例，提供了以下主要功能：
 * 1. LoRaWAN 1.0中继设备OTAA测试：验证LoRaWAN 1.0版本中继设备的OTAA流程
 *
 * 主要组件
 * ========
 * - test_lorawan_10(): 测试LoRaWAN 1.0版本中继设备的OTAA流程
 *
 * 关键流程
 * ========
 * 1. 中继设备OTAA测试执行流程:
 *    - 准备测试环境(租户、应用、设备配置、设备密钥等)
 *    - 创建中继设备和中继终端设备
 *    - 设置中继设备的Join-Request消息
 *    - 处理模拟的Join-Request上行消息
 *    - 验证Join-Accept响应的正确性
 *    - 验证中继设备会话的创建和密钥派生
 *    - 验证集成事件的发送
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备会话、下行消息等
 * - 协议版本: 测试专注于LoRaWAN 1.0版本的中继功能
 * - 安全考虑: OTAA涉及密钥派生和安全通信，测试应该验证安全机制的正确性
 * - 中继特性: 测试需要考虑中继设备的特殊属性和行为
 */

use std::time::Duration;

use uuid::Uuid;

use super::assert;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_keys, device_profile, gateway, tenant,
};
use crate::{gateway::backend as gateway_backend, integration, test, uplink};
use chirpstack_api::{common, gw, internal};
use lrwn::region::CommonName;
use lrwn::{AES128Key, DevAddr, EUI64};

#[tokio::test]
async fn test_lorawan_10() {
    let _guard = test::prepare().await;
    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    integration::mock::reset().await;
    gateway_backend::mock::reset().await;

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

    let dp_relay = device_profile::create(device_profile::DeviceProfile {
        name: "dp".into(),
        tenant_id: t.id,
        region: lrwn::region::CommonName::EU868,
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_2,
        reg_params_revision: lrwn::region::Revision::A,
        supports_otaa: true,
        is_relay: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([1, 1, 1, 1, 1, 1, 1, 1]),
        enabled_class: DeviceClass::A,
        ..Default::default()
    })
    .await
    .unwrap();

    let dk_dev = device_keys::create(device_keys::DeviceKeys {
        dev_eui: dev.dev_eui,
        nwk_key: AES128Key::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8]),
        ..Default::default()
    })
    .await
    .unwrap();

    let dev_relay = device::create(device::Device {
        name: "relay-device".into(),
        application_id: app.id,
        device_profile_id: dp_relay.id,
        dev_eui: EUI64::from_be_bytes([1, 1, 1, 1, 1, 1, 1, 2]),
        enabled_class: DeviceClass::A,
        dev_addr: Some(DevAddr::from_be_bytes([4, 3, 2, 1])),
        device_session: Some(
            internal::DeviceSession {
                mac_version: common::MacVersion::Lorawan102.into(),
                dev_addr: vec![4, 3, 2, 1],
                f_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                s_nwk_s_int_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                nwk_s_enc_key: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                f_cnt_up: 10,
                n_f_cnt_down: 5,
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

    let ds_relay = dev_relay.get_device_session().unwrap();

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
            dev_nonce: 1,
        }),
        mic: None,
    };
    jr_pl.set_join_request_mic(&dk_dev.nwk_key).unwrap();

    let mut ja_pl = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::JoinAccept,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::JoinAccept(lrwn::JoinAcceptPayload {
            home_netid: lrwn::NetID::from_be_bytes([0, 0, 0]),
            devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
            dl_settings: lrwn::DLSettings {
                rx2_dr: 0,
                rx1_dr_offset: 0,
                opt_neg: false,
            },
            rx_delay: 1,
            join_nonce: 0,
            cflist: None,
        }),
        mic: None,
    };
    ja_pl
        .set_join_accept_mic(
            lrwn::JoinType::Join,
            &EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            1,
            &dk_dev.nwk_key,
        )
        .unwrap();
    ja_pl.encrypt_join_accept_payload(&dk_dev.nwk_key).unwrap();

    let mut phy_relay_jr = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataUp,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: lrwn::DevAddr::from_be_bytes([4, 3, 2, 1]),
                f_cnt: 10,
                ..Default::default()
            },
            f_port: Some(226),
            frm_payload: Some(lrwn::FRMPayload::ForwardUplinkReq(lrwn::ForwardUplinkReq {
                metadata: lrwn::UplinkMetadata {
                    dr: 5,
                    snr: 10,
                    rssi: -120,
                    wor_channel: 0,
                },
                frequency: 868100000,
                payload: Box::new(jr_pl),
            })),
        }),
        mic: None,
    };
    phy_relay_jr
        .encrypt_frm_payload(&AES128Key::from_slice(&ds_relay.nwk_s_enc_key).unwrap())
        .unwrap();
    phy_relay_jr
        .set_uplink_data_mic(
            lrwn::MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &AES128Key::from_slice(&ds_relay.f_nwk_s_int_key).unwrap(),
            &AES128Key::from_slice(&ds_relay.s_nwk_s_int_key).unwrap(),
        )
        .unwrap();

    uplink::handle_uplink(
        CommonName::EU868,
        "eu868".into(),
        Uuid::new_v4(),
        gw::UplinkFrameSet {
            phy_payload: phy_relay_jr.to_vec().unwrap(),
            tx_info: Some(tx_info),
            rx_info: vec![rx_info],
        },
    )
    .await
    .unwrap();

    let mut phy_relay_ja = lrwn::PhyPayload {
        mhdr: lrwn::MHDR {
            m_type: lrwn::MType::UnconfirmedDataDown,
            major: lrwn::Major::LoRaWANR1,
        },
        payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
            fhdr: lrwn::FHDR {
                devaddr: lrwn::DevAddr::from_be_bytes([4, 3, 2, 1]),
                f_cnt: 5,
                f_ctrl: lrwn::FCtrl {
                    adr: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            f_port: Some(226),
            frm_payload: Some(lrwn::FRMPayload::ForwardDownlinkReq(
                lrwn::ForwardDownlinkReq {
                    payload: Box::new(ja_pl),
                },
            )),
        }),
        mic: None,
    };
    phy_relay_ja
        .encrypt_frm_payload(&AES128Key::from_slice(&ds_relay.nwk_s_enc_key).unwrap())
        .unwrap();
    phy_relay_ja
        .set_downlink_data_mic(
            lrwn::MACVersion::LoRaWAN1_0,
            10,
            &AES128Key::from_slice(&ds_relay.s_nwk_s_int_key).unwrap(),
        )
        .unwrap();

    let assertions = vec![
        assert::device_session(
            dev.dev_eui,
            internal::DeviceSession {
                dev_addr: vec![1, 2, 3, 4],
                mac_version: common::MacVersion::Lorawan102.into(),
                f_nwk_s_int_key: vec![
                    146, 184, 94, 251, 180, 89, 48, 96, 236, 112, 106, 181, 94, 25, 215, 162,
                ],
                s_nwk_s_int_key: vec![
                    146, 184, 94, 251, 180, 89, 48, 96, 236, 112, 106, 181, 94, 25, 215, 162,
                ],
                nwk_s_enc_key: vec![
                    146, 184, 94, 251, 180, 89, 48, 96, 236, 112, 106, 181, 94, 25, 215, 162,
                ],
                app_s_key: Some(common::KeyEnvelope {
                    kek_label: "".to_string(),
                    aes_key: vec![
                        181, 55, 181, 113, 220, 19, 233, 58, 156, 54, 209, 48, 209, 201, 73, 33,
                    ],
                }),
                rx1_delay: 1,
                rx2_frequency: 869525000,
                enabled_uplink_channel_indices: vec![0, 1, 2],
                nb_trans: 1,
                region_config_id: "eu868".to_string(),
                class_b_ping_slot_nb: 1,
                ..Default::default()
            }
            .into(),
        ),
        assert::downlink_frame(gw::DownlinkFrame {
            items: vec![
                gw::DownlinkFrameItem {
                    phy_payload: phy_relay_ja.to_vec().unwrap(),
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
                            parameters: Some(gw::timing::Parameters::Delay(gw::DelayTimingInfo {
                                delay: Some(Duration::from_secs(1).into()),
                            })),
                        }),
                        ..Default::default()
                    }),
                },
                gw::DownlinkFrameItem {
                    phy_payload: phy_relay_ja.to_vec().unwrap(),
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
                            parameters: Some(gw::timing::Parameters::Delay(gw::DelayTimingInfo {
                                delay: Some(Duration::from_secs(2).into()),
                            })),
                        }),
                        ..Default::default()
                    }),
                },
            ],
            gateway_id: "0102030405060708".to_string(),
            ..Default::default()
        }),
    ];

    for assert in &assertions {
        assert().await;
    }
}

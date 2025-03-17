/*
 * 模块概述
 * ========
 * Class B测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，专门用于测试
 * LoRaWAN Class B设备的通信功能。Class B是LoRaWAN设备的一种工作模式，除了Class A的
 * 基本功能外，还允许设备在预定的时间点(ping slot)接收下行消息，从而实现更低的下行延迟。
 * 
 * 该模块实现了一系列测试用例，覆盖了Class B设备通信的各个方面，包括上行消息处理、
 * ping slot下行消息调度、beacon同步等。通过这些测试，确保ChirpStack能够正确实现
 * LoRaWAN协议规范中关于Class B设备的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(class_b_test.rs)实现了Class B设备的测试用例，提供了以下主要功能：
 * 1. Class B上行测试：验证Class B设备的上行消息处理
 * 2. Class B下行调度测试：验证Class B设备的ping slot下行消息调度
 *
 * 主要组件
 * ========
 * - UplinkTest结构体: 定义上行测试用例的参数和断言
 * - DownlinkTest结构体: 定义下行测试用例的参数和断言
 * - test_uplink(): 测试Class B设备的上行消息处理
 * - test_downlink_scheduler(): 测试Class B设备的下行消息调度
 * - run_uplink_test(): 运行上行测试用例的辅助函数
 * - run_scheduler_test(): 运行下行调度测试用例的辅助函数
 *
 * 关键流程
 * ========
 * 1. Class B上行测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置测试参数(设备会话、上行消息等)
 *    - 处理模拟的上行消息
 *    - 验证设备状态和响应
 *
 * 2. Class B下行调度测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置测试参数(设备会话、下行队列项等)
 *    - 触发下行消息调度
 *    - 验证ping slot下行消息的正确调度
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备状态、下行消息等
 * - 时间同步: Class B设备需要与网络服务器时间同步，测试应验证这一点
 * - Ping Slot: 测试应验证ping slot的正确计算和使用
 * - Beacon: 测试应考虑beacon周期和ping slot的关系
 */

use uuid::Uuid;

use super::assert;
use crate::gpstime::ToGpsTime;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_gateway, device_profile, device_queue, fields, gateway, reset_redis, tenant,
};
use crate::{
    config, downlink, downlink::classb, gateway::backend as gateway_backend, integration, test,
    uplink,
};
use chirpstack_api::{common, gw, internal};
use lrwn::region::CommonName;
use lrwn::{DevAddr, EUI64};

struct UplinkTest {
    name: String,
    dev_eui: EUI64,
    device_queue_items: Vec<device_queue::DeviceQueueItem>,
    device_session: Option<internal::DeviceSession>,
    tx_info: gw::UplinkTxInfo,
    rx_info: gw::UplinkRxInfo,
    phy_payload: lrwn::PhyPayload,
    assert: Vec<assert::Validator>,
}

struct DownlinkTest {
    name: String,
    dev_eui: EUI64,
    device_queue_items: Vec<device_queue::DeviceQueueItem>,
    device_session: Option<internal::DeviceSession>,
    device_gateway_rx_info: Option<internal::DeviceGatewayRxInfo>,
    assert: Vec<assert::Validator>,
}

#[tokio::test]
async fn test_uplink() {
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
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_4,
        reg_params_revision: lrwn::region::Revision::RP002_1_0_3,
        supports_otaa: true,
        supports_class_b: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::A,
        dev_addr: Some(DevAddr::from_be_bytes([1, 2, 3, 4])),
        ..Default::default()
    })
    .await
    .unwrap();

    let rx_info = gw::UplinkRxInfo {
        gateway_id: gw.gateway_id.to_string(),
        ..Default::default()
    };

    let mut tx_info = gw::UplinkTxInfo {
        frequency: 868100000,
        ..Default::default()
    };
    uplink::helpers::set_uplink_modulation("eu868", &mut tx_info, 0).unwrap();

    let ds = internal::DeviceSession {
        mac_version: common::MacVersion::Lorawan104.into(),
        dev_addr: vec![1, 2, 3, 4],
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
        rx2_frequency: 869525000,
        region_config_id: "eu868".into(),
        ..Default::default()
    };

    let mut conf = (*config::get()).clone();
    conf.regions[0].network.class_b.ping_slot_dr = 2;
    conf.regions[0].network.class_b.ping_slot_frequency = 868300000;
    config::set(conf);

    // trigger beacon locked
    run_uplink_test(&UplinkTest {
        name: "trigger beacon locked".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![],
        device_session: Some(ds.clone()),
        tx_info: tx_info.clone(),
        rx_info: rx_info.clone(),
        phy_payload: lrwn::PhyPayload {
            mhdr: lrwn::MHDR {
                m_type: lrwn::MType::UnconfirmedDataUp,
                major: lrwn::Major::LoRaWANR1,
            },
            payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
                fhdr: lrwn::FHDR {
                    devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
                    f_cnt: 8,
                    f_ctrl: lrwn::FCtrl {
                        class_b: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                f_port: None,
                frm_payload: None,
            }),
            mic: Some([241, 100, 207, 79]),
        },
        assert: vec![
            assert::f_cnt_up(dev.dev_eui, 9),
            assert::enabled_class(dev.dev_eui, DeviceClass::B),
        ],
    })
    .await;

    // trigger beacon unlocked
    run_uplink_test(&UplinkTest {
        name: "trigger beacon locked".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![],
        device_session: Some(ds.clone()),
        tx_info: tx_info.clone(),
        rx_info: rx_info.clone(),
        phy_payload: lrwn::PhyPayload {
            mhdr: lrwn::MHDR {
                m_type: lrwn::MType::UnconfirmedDataUp,
                major: lrwn::Major::LoRaWANR1,
            },
            payload: lrwn::Payload::MACPayload(lrwn::MACPayload {
                fhdr: lrwn::FHDR {
                    devaddr: lrwn::DevAddr::from_be_bytes([1, 2, 3, 4]),
                    f_cnt: 8,
                    f_ctrl: lrwn::FCtrl {
                        class_b: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                f_port: None,
                frm_payload: None,
            }),
            mic: Some([137, 180, 12, 148]),
        },
        assert: vec![
            assert::f_cnt_up(dev.dev_eui, 9),
            assert::enabled_class(dev.dev_eui, DeviceClass::A),
        ],
    })
    .await;
}

#[tokio::test]
async fn test_downlink_scheduler() {
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
        mac_version: lrwn::region::MacVersion::LORAWAN_1_0_4,
        reg_params_revision: lrwn::region::Revision::RP002_1_0_3,
        supports_otaa: true,
        supports_class_b: true,
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
        dev_addr: Some(DevAddr::from_be_bytes([1, 2, 3, 4])),
        ..Default::default()
    })
    .await
    .unwrap();

    let ds = internal::DeviceSession {
        mac_version: common::MacVersion::Lorawan104.into(),
        dev_addr: vec![1, 2, 3, 4],
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
        rx2_frequency: 869525000,
        region_config_id: "eu868".into(),
        class_b_ping_slot_freq: 868300000,
        class_b_ping_slot_dr: 2,
        class_b_ping_slot_nb: 1,
        ..Default::default()
    };

    let device_gateway_rx_info = internal::DeviceGatewayRxInfo {
        dev_eui: dev.dev_eui.to_vec(),
        items: vec![internal::DeviceGatewayRxInfoItem {
            gateway_id: gw.gateway_id.to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let now_gps_ts = chrono::Utc::now().to_gps_time() + chrono::Duration::try_seconds(1).unwrap();
    let ping_slot_ts = classb::get_next_ping_slot_after(
        now_gps_ts,
        &DevAddr::from_slice(&ds.dev_addr).unwrap(),
        ds.class_b_ping_slot_nb as usize,
    )
    .unwrap();

    run_scheduler_test(&DownlinkTest {
        name: "class-b downlink".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![device_queue::DeviceQueueItem {
            id: Uuid::nil().into(),
            dev_eui: dev.dev_eui,
            f_port: 10,
            data: vec![1, 2, 3],
            ..Default::default()
        }],
        device_session: Some(ds.clone()),
        device_gateway_rx_info: Some(device_gateway_rx_info.clone()),
        assert: vec![
            assert::f_cnt_up(dev.dev_eui, 8),
            assert::n_f_cnt_down(dev.dev_eui, 5),
            assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: "0102030405060708".into(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![96, 4, 3, 2, 1, 128, 5, 0, 10, 115, 46, 73, 41, 113, 46, 49],
                    tx_info_legacy: None,
                    tx_info: Some(gw::DownlinkTxInfo {
                        frequency: 868300000,
                        power: 16,
                        modulation: Some(gw::Modulation {
                            parameters: Some(gw::modulation::Parameters::Lora(
                                gw::LoraModulationInfo {
                                    bandwidth: 125000,
                                    spreading_factor: 10,
                                    code_rate: gw::CodeRate::Cr45.into(),
                                    polarization_inversion: true,
                                    ..Default::default()
                                },
                            )),
                        }),
                        timing: Some(gw::Timing {
                            parameters: Some(gw::timing::Parameters::GpsEpoch(
                                gw::GpsEpochTimingInfo {
                                    time_since_gps_epoch: Some(pbjson_types::Duration::from(
                                        ping_slot_ts.to_std().unwrap(),
                                    )),
                                },
                            )),
                        }),
                        ..Default::default()
                    }),
                }],
                ..Default::default()
            }),
        ],
    })
    .await;

    run_scheduler_test(&DownlinkTest {
        name: "scheduler_run_after has not yet expired".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![device_queue::DeviceQueueItem {
            id: Uuid::nil().into(),
            dev_eui: dev.dev_eui,
            f_port: 10,
            data: vec![1, 2, 3],
            ..Default::default()
        }],
        device_session: Some(ds.clone()),
        device_gateway_rx_info: Some(device_gateway_rx_info.clone()),
        assert: vec![assert::no_downlink_frame()],
    })
    .await;

    // remove the schedule run after
    device::partial_update(
        dev.dev_eui,
        &device::DeviceChangeset {
            scheduler_run_after: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    run_scheduler_test(&DownlinkTest {
        name: "class-b downlink with more data".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![
            device_queue::DeviceQueueItem {
                id: Uuid::nil().into(),
                dev_eui: dev.dev_eui,
                f_port: 10,
                data: vec![1, 2, 3],
                ..Default::default()
            },
            device_queue::DeviceQueueItem {
                id: Uuid::new_v4().into(),
                dev_eui: dev.dev_eui,
                f_port: 10,
                data: vec![1, 2, 3, 4],
                ..Default::default()
            },
        ],
        device_session: Some(ds.clone()),
        device_gateway_rx_info: Some(device_gateway_rx_info.clone()),
        assert: vec![
            assert::f_cnt_up(dev.dev_eui, 8),
            assert::n_f_cnt_down(dev.dev_eui, 5),
            assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: "0102030405060708".into(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![
                        96, 4, 3, 2, 1, 144, 5, 0, 10, 115, 46, 73, 218, 230, 215, 91,
                    ],
                    tx_info_legacy: None,
                    tx_info: Some(gw::DownlinkTxInfo {
                        frequency: 868300000,
                        power: 16,
                        modulation: Some(gw::Modulation {
                            parameters: Some(gw::modulation::Parameters::Lora(
                                gw::LoraModulationInfo {
                                    bandwidth: 125000,
                                    spreading_factor: 10,
                                    code_rate: gw::CodeRate::Cr45.into(),
                                    polarization_inversion: true,
                                    ..Default::default()
                                },
                            )),
                        }),
                        timing: Some(gw::Timing {
                            parameters: Some(gw::timing::Parameters::GpsEpoch(
                                gw::GpsEpochTimingInfo {
                                    time_since_gps_epoch: Some(pbjson_types::Duration::from(
                                        ping_slot_ts.to_std().unwrap(),
                                    )),
                                },
                            )),
                        }),
                        ..Default::default()
                    }),
                }],
                ..Default::default()
            }),
        ],
    })
    .await;
}

async fn run_uplink_test(t: &UplinkTest) {
    println!("> {}", t.name);

    reset_redis().await.unwrap();

    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    integration::mock::reset().await;
    gateway_backend::mock::reset().await;

    device_queue::flush_for_dev_eui(&t.dev_eui).await.unwrap();
    device::partial_update(
        t.dev_eui,
        &device::DeviceChangeset {
            device_session: Some(
                t.device_session
                    .as_ref()
                    .map(fields::DeviceSession::from)
                    .clone(),
            ),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    for qi in &t.device_queue_items {
        let _ = device_queue::enqueue_item(qi.clone()).await.unwrap();
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
}

async fn run_scheduler_test(t: &DownlinkTest) {
    println!("> {}", t.name);

    reset_redis().await.unwrap();

    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    integration::mock::reset().await;
    gateway_backend::mock::reset().await;
    device_queue::flush_for_dev_eui(&t.dev_eui).await.unwrap();
    device::partial_update(
        t.dev_eui,
        &device::DeviceChangeset {
            device_session: Some(
                t.device_session
                    .as_ref()
                    .map(fields::DeviceSession::from)
                    .clone(),
            ),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    if let Some(rx_info) = &t.device_gateway_rx_info {
        device_gateway::save_rx_info(rx_info).await.unwrap();
    }

    for qi in &t.device_queue_items {
        let _ = device_queue::enqueue_item(qi.clone()).await.unwrap();
    }

    downlink::scheduler::schedule_device_queue_batch(1)
        .await
        .unwrap();

    for assert in &t.assert {
        assert().await;
    }
}

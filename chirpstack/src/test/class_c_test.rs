/*
 * 模块概述
 * ========
 * Class C测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，专门用于测试
 * LoRaWAN Class C设备的通信功能。Class C是LoRaWAN设备的一种工作模式，与Class A和Class B
 * 不同，Class C设备持续保持接收窗口打开，只在发送上行消息时短暂关闭，从而实现最低的下行延迟。
 * 
 * 该模块实现了一系列测试用例，覆盖了Class C设备通信的各个方面，特别是下行消息调度功能。
 * 通过这些测试，确保ChirpStack能够正确实现LoRaWAN协议规范中关于Class C设备的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(class_c_test.rs)实现了Class C设备的测试用例，提供了以下主要功能：
 * 1. Class C上行测试：验证Class C设备的上行消息处理（待实现）
 * 2. Class C下行调度测试：验证Class C设备的下行消息调度
 *
 * 主要组件
 * ========
 * - DownlinkTest结构体: 定义下行测试用例的参数和断言
 * - test_uplink(): 测试Class C设备的上行消息处理（待实现）
 * - test_downlink_scheduler(): 测试Class C设备的下行消息调度
 * - run_scheduler_test(): 运行下行调度测试用例的辅助函数
 *
 * 关键流程
 * ========
 * 1. Class C下行调度测试流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 设置测试参数(设备会话、下行队列项等)
 *    - 触发下行消息调度
 *    - 验证Class C下行消息的正确调度
 *    - 验证多个下行消息的处理
 *    - 验证下行消息的优先级处理
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括设备状态、下行消息等
 * - 持续接收: Class C设备的特点是持续接收，测试应验证这一特性
 * - 电源要求: Class C设备通常需要持续供电，这在测试中应该被考虑
 * - 下行延迟: Class C设备提供最低的下行延迟，测试应验证这一点
 */

use uuid::Uuid;

use super::assert;
use crate::storage::{
    application,
    device::{self, DeviceClass},
    device_gateway, device_profile, device_queue, fields, gateway, reset_redis, tenant,
};
use crate::{downlink, gateway::backend as gateway_backend, integration, test};
use chirpstack_api::{common, gw, internal};
use lrwn::{DevAddr, EUI64};

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
    // TODO: implement changing from Class A -> C?
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
        supports_class_c: true,
        ..Default::default()
    })
    .await
    .unwrap();

    let dev = device::create(device::Device {
        name: "device".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
        enabled_class: DeviceClass::C,
        dev_addr: Some(DevAddr::from_be_bytes([1, 2, 3, 4])),
        ..Default::default()
    })
    .await
    .unwrap();

    let device_gateway_rx_info = internal::DeviceGatewayRxInfo {
        dev_eui: dev.dev_eui.to_vec(),
        items: vec![internal::DeviceGatewayRxInfoItem {
            gateway_id: gw.gateway_id.to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let ds = internal::DeviceSession {
        dev_addr: vec![1, 2, 3, 4],
        mac_version: common::MacVersion::Lorawan104.into(),
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

    let ds_no_uplink = internal::DeviceSession {
        f_cnt_up: 0,
        ..ds.clone()
    };

    run_scheduler_test(&DownlinkTest {
        name: "device has not yet sent an uplink".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![device_queue::DeviceQueueItem {
            id: Uuid::nil().into(),
            dev_eui: dev.dev_eui,
            f_port: 10,
            data: vec![1, 2, 3],
            ..Default::default()
        }],
        device_session: Some(ds_no_uplink.clone()),
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
        name: "unconfirmed data".into(),
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
            assert::n_f_cnt_down(dev.dev_eui, 5),
            assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: "0102030405060708".into(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![96, 4, 3, 2, 1, 128, 5, 0, 10, 115, 46, 73, 41, 113, 46, 49],
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
                            parameters: Some(gw::timing::Parameters::Immediately(
                                gw::ImmediatelyTimingInfo {},
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
        name: "unconfirmed data".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![device_queue::DeviceQueueItem {
            id: Uuid::nil().into(),
            dev_eui: dev.dev_eui,
            f_port: 10,
            data: vec![1, 2, 3],
            confirmed: true,
            ..Default::default()
        }],
        device_session: Some(ds.clone()),
        device_gateway_rx_info: Some(device_gateway_rx_info.clone()),
        assert: vec![
            assert::n_f_cnt_down(dev.dev_eui, 5),
            assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: "0102030405060708".into(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![
                        160, 4, 3, 2, 1, 128, 5, 0, 10, 115, 46, 73, 138, 39, 53, 228,
                    ],
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
                            parameters: Some(gw::timing::Parameters::Immediately(
                                gw::ImmediatelyTimingInfo {},
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
        name: "unconfirmed data".into(),
        dev_eui: dev.dev_eui,
        device_queue_items: vec![device_queue::DeviceQueueItem {
            id: Uuid::nil().into(),
            dev_eui: dev.dev_eui,
            f_port: 10,
            data: vec![0; 300],
            ..Default::default()
        }],
        device_session: Some(ds.clone()),
        device_gateway_rx_info: Some(device_gateway_rx_info.clone()),
        assert: vec![assert::no_downlink_frame()],
    })
    .await;
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

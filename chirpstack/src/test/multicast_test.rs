/*
 * 模块概述
 * ========
 * 多播(Multicast)测试模块是ChirpStack LoRaWAN网络服务器测试框架的重要组成部分，专门用于测试
 * LoRaWAN多播功能。多播是LoRaWAN网络中的一项重要功能，它允许网络服务器同时向多个设备发送
 * 相同的下行消息，从而提高网络效率，特别适用于固件更新、批量命令等场景。
 * 
 * 该模块实现了一系列测试用例，覆盖了多播功能的各个方面，包括多播组创建、设备添加、下行消息
 * 调度等。通过这些测试，确保ChirpStack能够正确实现LoRaWAN协议规范中关于多播的所有要求。
 * 
 * 测试采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(multicast_test.rs)实现了多播功能的测试用例，提供了以下主要功能：
 * 1. 多播组创建测试：验证多播组的创建和配置
 * 2. 多播设备添加测试：验证设备添加到多播组的功能
 * 3. 多播下行消息测试：验证多播下行消息的调度和发送
 *
 * 主要组件
 * ========
 * - MulticastTest结构体: 定义测试用例的参数和断言
 * - test_multicast(): 测试多播功能
 * - run_scheduler_test(): 运行多播调度测试的辅助函数
 *
 * 关键流程
 * ========
 * 1. 多播测试执行流程:
 *    - 准备测试环境(租户、应用、设备配置等)
 *    - 创建多播组和设备
 *    - 将设备添加到多播组
 *    - 设置多播下行队列项
 *    - 触发多播下行消息调度
 *    - 验证多播下行消息的正确发送
 *
 * 注意事项
 * ========
 * - 测试隔离: 每个测试用例都应该是独立的，不依赖其他测试的状态
 * - 资源清理: 测试应该清理它创建的所有资源，避免影响其他测试
 * - 断言全面性: 断言应该全面验证测试结果，包括多播组状态、下行消息等
 * - 设备兼容性: 只有支持Class B或Class C的设备才能加入多播组
 * - 网关选择: 多播组需要选择合适的网关来发送下行消息
 * - 频率限制: 多播下行消息需要遵守区域性的频率和占空比限制
 */

use chrono::{Duration, Utc};

use super::assert;
use crate::storage::{
    application, device, device_gateway, device_profile, fields, gateway, multicast, tenant,
};
use crate::{downlink, gateway::backend as gateway_backend, integration, test};
use chirpstack_api::{gw, internal};
use lrwn::{AES128Key, DevAddr, EUI64};

struct MulticastTest {
    name: String,
    multicast_group: multicast::MulticastGroup,
    multicast_group_queue_items: Vec<multicast::MulticastGroupQueueItem>,
    assert: Vec<assert::Validator>,
}

#[tokio::test]
async fn test_multicast() {
    let _guard = test::prepare().await;

    // tenant
    let t = tenant::create(tenant::Tenant {
        name: "test-tenant".into(),
        can_have_gateways: true,
        ..Default::default()
    })
    .await
    .unwrap();

    // gateway
    let gw = gateway::create(gateway::Gateway {
        tenant_id: t.id,
        gateway_id: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
        name: "test-gw".into(),
        properties: fields::KeyValue::new(
            [("region_config_id".to_string(), "eu868".to_string())]
                .iter()
                .cloned()
                .collect(),
        ),
        stats_interval_secs: 30,
        last_seen_at: Some(Utc::now()),
        ..Default::default()
    })
    .await
    .unwrap();

    // application
    let app = application::create(application::Application {
        name: "test-app".into(),
        tenant_id: t.id,
        ..Default::default()
    })
    .await
    .unwrap();

    // device-profile
    let dp = device_profile::create(device_profile::DeviceProfile {
        name: "test-dp".into(),
        tenant_id: t.id,
        ..Default::default()
    })
    .await
    .unwrap();

    // device
    let d = device::create(device::Device {
        name: "test-dev".into(),
        application_id: app.id,
        device_profile_id: dp.id,
        dev_eui: EUI64::from_be_bytes([8, 7, 6, 5, 4, 3, 2, 1]),
        ..Default::default()
    })
    .await
    .unwrap();

    // multicast group
    let mg = multicast::create(multicast::MulticastGroup {
        application_id: app.id,
        name: "test-mg".into(),
        mc_addr: DevAddr::from_be_bytes([1, 2, 3, 4]),
        mc_nwk_s_key: AES128Key::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8]),
        mc_app_s_key: AES128Key::from_bytes([2, 2, 3, 4, 5, 6, 7, 8, 1, 2, 3, 4, 5, 6, 7, 8]),
        f_cnt: 10,
        group_type: "C".into(),
        dr: 3,
        frequency: 868300000,
        class_b_ping_slot_nb_k: 0,
        ..Default::default()
    })
    .await
    .unwrap();
    multicast::add_device(&mg.id.into(), &d.dev_eui)
        .await
        .unwrap();

    // device <> gateway
    device_gateway::save_rx_info(&internal::DeviceGatewayRxInfo {
        dev_eui: d.dev_eui.to_vec(),
        items: vec![internal::DeviceGatewayRxInfoItem {
            gateway_id: gw.gateway_id.to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    })
    .await
    .unwrap();

    let tests = vec![
        MulticastTest {
            name: "nothing in queue".into(),
            multicast_group: mg.clone(),
            multicast_group_queue_items: vec![],
            assert: vec![assert::no_downlink_frame()],
        },
        MulticastTest {
            name: "one item in queue".into(),
            multicast_group: mg.clone(),
            multicast_group_queue_items: vec![multicast::MulticastGroupQueueItem {
                multicast_group_id: mg.id,
                f_port: 5,
                data: vec![1, 2, 3],
                ..Default::default()
            }],
            assert: vec![assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: gw.gateway_id.to_string(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![
                        96, 4, 3, 2, 1, 0, 10, 0, 5, 161, 250, 255, 42, 110, 141, 200,
                    ],
                    tx_info_legacy: None,
                    tx_info: Some(gw::DownlinkTxInfo {
                        frequency: 868300000,
                        power: 16,
                        modulation: Some(gw::Modulation {
                            parameters: Some(gw::modulation::Parameters::Lora(
                                gw::LoraModulationInfo {
                                    bandwidth: 125000,
                                    spreading_factor: 9,
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
            })],
        },
        MulticastTest {
            name: "two items in queue".into(),
            multicast_group: mg.clone(),
            multicast_group_queue_items: vec![
                multicast::MulticastGroupQueueItem {
                    multicast_group_id: mg.id,
                    f_port: 5,
                    data: vec![1, 2, 3],
                    ..Default::default()
                },
                multicast::MulticastGroupQueueItem {
                    multicast_group_id: mg.id,
                    f_port: 6,
                    data: vec![1, 2, 3],
                    ..Default::default()
                },
            ],
            assert: vec![assert::downlink_frame(gw::DownlinkFrame {
                gateway_id: gw.gateway_id.to_string(),
                items: vec![gw::DownlinkFrameItem {
                    phy_payload: vec![
                        96, 4, 3, 2, 1, 0, 10, 0, 5, 161, 250, 255, 42, 110, 141, 200,
                    ],
                    tx_info_legacy: None,
                    tx_info: Some(gw::DownlinkTxInfo {
                        frequency: 868300000,
                        power: 16,
                        modulation: Some(gw::Modulation {
                            parameters: Some(gw::modulation::Parameters::Lora(
                                gw::LoraModulationInfo {
                                    bandwidth: 125000,
                                    spreading_factor: 9,
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
            })],
        },
        MulticastTest {
            name: "item discarded because of payload size".into(),
            multicast_group: mg.clone(),
            multicast_group_queue_items: vec![multicast::MulticastGroupQueueItem {
                multicast_group_id: mg.id,
                f_port: 5,
                data: vec![2; 300],
                ..Default::default()
            }],
            assert: vec![assert::no_downlink_frame()],
        },
        MulticastTest {
            name: "item discarded because it has expired".into(),
            multicast_group: mg.clone(),
            multicast_group_queue_items: vec![multicast::MulticastGroupQueueItem {
                multicast_group_id: mg.id,
                f_port: 5,
                data: vec![1, 2, 3],
                expires_at: Some(Utc::now() - Duration::seconds(10)),
                ..Default::default()
            }],
            assert: vec![assert::no_downlink_frame()],
        },
    ];

    for tst in &tests {
        run_scheduler_test(tst).await;
    }
}

async fn run_scheduler_test(t: &MulticastTest) {
    println!("> {}", t.name);

    integration::set_mock().await;
    gateway_backend::set_backend("eu868", Box::new(gateway_backend::mock::Backend {})).await;

    // overwrite multicast-group to deal with frame-counter increments
    multicast::update(t.multicast_group.clone()).await.unwrap();

    // set multicast-group queue
    multicast::flush_queue(&t.multicast_group.id).await.unwrap();
    for qi in &t.multicast_group_queue_items {
        let _ = downlink::multicast::enqueue(qi.clone()).await.unwrap();
    }

    downlink::scheduler::schedule_multicast_group_queue_batch(1)
        .await
        .unwrap();

    for assert in &t.assert {
        assert().await;
    }
}

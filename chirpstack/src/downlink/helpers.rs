/*
 * 模块概述
 * ========
 * 下行链路辅助(Downlink Helpers)模块是ChirpStack LoRaWAN网络服务器下行链路处理框架的通用工具组件，
 * 提供了一系列用于下行链路处理的辅助函数和工具。这些工具被下行链路处理框架的其他模块广泛使用，
 * 用于执行常见的下行链路相关操作，如网关选择、发送参数设置等。
 * 
 * 该模块通过提供标准化的辅助函数，确保了下行链路处理的一致性和可靠性，同时也简化了其他模块的
 * 实现复杂度。它处理了许多与下行链路相关的技术细节，使得其他模块可以专注于其特定的业务逻辑。
 * 
 * 本模块的功能涵盖了从网关选择到发送参数设置的多个方面，是整个下行链路处理框架的基础支持组件。
 *
 * 文件功能
 * ========
 * 本文件(helpers.rs)实现了下行链路处理的辅助函数，提供了以下主要功能：
 * 1. 下行链路网关选择，基于信号质量和租户权限
 * 2. 下行链路发送参数设置，如数据速率、频率、发送功率等
 * 3. 网关过滤，根据租户ID和私有下行设置
 * 4. 信号质量评估，用于选择最佳下行网关
 * 5. 数据速率和调制方式转换，用于设置下行链路参数
 *
 * 主要组件
 * ========
 * - select_downlink_gateway(): 选择用于下行链路的最佳网关
 *   - 根据租户ID和私有下行设置过滤网关
 *   - 基于信号质量(SNR/RSSI)排序网关
 *   - 应用最小SNR边界选择合适的网关
 * 
 * - set_tx_info_data_rate(): 设置下行链路发送信息的数据速率
 *   - 根据区域配置和数据速率索引设置调制参数
 *   - 支持LoRa和FSK调制方式
 *   - 设置带宽、扩频因子、编码率等参数
 *
 * 关键流程
 * ========
 * 1. 下行链路网关选择流程:
 *    - 根据租户ID和私有下行设置过滤网关列表
 *    - 计算每个网关的信号质量
 *    - 根据信号质量排序网关列表
 *    - 应用最小SNR边界选择合适的网关
 *    - 如果有多个满足条件的网关，随机选择一个
 *    - 如果没有满足SNR边界的网关，选择信号最好的网关
 *
 * 2. 下行链路参数设置流程:
 *    - 根据数据速率索引获取调制参数
 *    - 根据调制方式(LoRa/FSK)设置不同的参数
 *    - 对于LoRa，设置带宽、扩频因子、编码率
 *    - 对于FSK，设置比特率和频率偏移
 *
 * 注意事项
 * ========
 * - 租户权限: 必须考虑网关的租户归属和私有下行设置
 * - 信号质量: 网关选择应优先考虑信号质量好的网关
 * - 负载均衡: 在多个信号质量相近的网关中随机选择，实现负载均衡
 * - 区域规范: 参数设置必须符合不同区域的规范要求
 * - 资源优化: 网关选择应考虑资源利用效率
 * - 可靠性: 在极端情况下（如所有网关信号质量都不理想）仍能选择最佳可用网关
 * - 兼容性: 参数设置必须考虑不同网关和设备的兼容性
 */

use std::str::FromStr;

use anyhow::Result;
use rand::seq::SliceRandom;
use uuid::Uuid;

use chirpstack_api::{gw, internal};
use lrwn::region::DataRateModulation;

use crate::config;
use crate::region;

// Returns the gateway to use for downlink.
// It will filter out private gateways (gateways from a different tenant ID,
// that do not allow downlinks). The result will be sorted based on SNR / RSSI.
// The returned value is:
//  * A random item from the elements with an SNR > minSNR
//  * The first item of the sorted slice (failing the above)
//  * An error in case no gateways are available
pub fn select_downlink_gateway(
    tenant_id: Option<Uuid>,
    region_config_id: &str,
    min_snr_margin: f32,
    rx_info: &mut internal::DeviceGatewayRxInfo,
) -> Result<internal::DeviceGatewayRxInfoItem> {
    rx_info.items.retain(|rx_info| {
        if let Some(tenant_id) = &tenant_id {
            if tenant_id.as_bytes().to_vec() == rx_info.tenant_id {
                // The tenant is the same as the gateway tenant.
                true
            } else {
                // If tenant_id is different, filter out rx_info elements that have
                // is_private_down=true.
                !rx_info.is_private_down
            }
        } else {
            // If tenant_id is None, filter out rx_info elements that have
            // is_private_down=true.
            !rx_info.is_private_down
        }
    });

    if rx_info.items.is_empty() {
        return Err(anyhow!(
            "RxInfo set is empty after applying filters, no downlink gateway available"
        ));
    }

    let region_conf = region::get(region_config_id)?;

    let dr = region_conf.get_data_rate(rx_info.dr as u8)?;
    let mut required_snr: Option<f32> = None;
    if let DataRateModulation::Lora(dr) = dr {
        required_snr = Some(config::get_required_snr_for_sf(dr.spreading_factor)?);
    }

    // sort items by SNR or if SNR is equal between A and B, by RSSI.
    rx_info.items.sort_by(|a, b| {
        if a.lora_snr == b.lora_snr {
            return b.rssi.partial_cmp(&a.rssi).unwrap();
        }
        b.lora_snr.partial_cmp(&a.lora_snr).unwrap()
    });

    let mut new_items = Vec::new();
    for item in &rx_info.items {
        if let Some(required_snr) = required_snr {
            if item.lora_snr - required_snr >= min_snr_margin {
                new_items.push(item.clone());
            }
        }
    }

    // Return a random item from the new_items slice (filtered by min_snr_margin).
    // If new_items is empty, then choose will return None and we return the first item from
    // rx_info.item.
    Ok(match new_items.choose(&mut rand::thread_rng()) {
        Some(v) => v.clone(),
        None => rx_info.items[0].clone(),
    })
}

pub fn set_tx_info_data_rate(
    tx_info: &mut chirpstack_api::gw::DownlinkTxInfo,
    dr: &DataRateModulation,
) -> Result<()> {
    match dr {
        DataRateModulation::Lora(v) => {
            tx_info.modulation = Some(gw::Modulation {
                parameters: Some(gw::modulation::Parameters::Lora(gw::LoraModulationInfo {
                    bandwidth: v.bandwidth,
                    spreading_factor: v.spreading_factor as u32,
                    code_rate: gw::CodeRate::from_str(&v.coding_rate)
                        .map_err(|e| anyhow!("{}", e))?
                        .into(),
                    polarization_inversion: true,
                    code_rate_legacy: "".into(),
                    preamble: 0,
                    no_crc: false,
                })),
            });
        }
        DataRateModulation::Fsk(v) => {
            tx_info.modulation = Some(gw::Modulation {
                parameters: Some(gw::modulation::Parameters::Fsk(gw::FskModulationInfo {
                    datarate: v.bitrate,
                    frequency_deviation: v.bitrate / 2, // see: https://github.com/brocaar/chirpstack-gateway-bridge/issues/16
                })),
            });
        }
        DataRateModulation::LrFhss(_) => {
            return Err(anyhow!("LR-FHSS is not supported for downlink"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::storage::tenant;
    use crate::test;

    struct Test {
        min_snr_margin: f32,
        tenant_id: Option<Uuid>,
        rx_info: internal::DeviceGatewayRxInfo,
        expected_gws: Vec<Vec<u8>>,
    }

    #[tokio::test]
    async fn test_select_downlink_gateway() {
        let _guard = test::prepare().await;

        let t = tenant::create(tenant::Tenant {
            name: "test-tenant".into(),
            ..Default::default()
        })
        .await
        .unwrap();

        let tests = vec![
            // single item
            Test {
                tenant_id: None,
                min_snr_margin: 0.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    dr: 0,
                    items: vec![internal::DeviceGatewayRxInfoItem {
                        lora_snr: -5.0,
                        gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]],
            },
            // two items, below min snr
            Test {
                tenant_id: None,
                min_snr_margin: 5.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    dr: 2, // -15 is required
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -12.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -11.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]],
            },
            // two items, one below min snr
            Test {
                tenant_id: None,
                min_snr_margin: 5.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    dr: 2, // -15 is required
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -12.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -10.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]],
            },
            // four items, two below min snr
            Test {
                tenant_id: None,
                min_snr_margin: 5.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    dr: 2, // -15 is required
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -12.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -11.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -10.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            lora_snr: -9.0,
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![
                    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
                    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
                ],
            },
            // is_private_down is set, first gateway matches tenant.
            Test {
                tenant_id: Some(t.id.into()),
                min_snr_margin: 0.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            is_private_down: true,
                            tenant_id: t.id.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            is_private_down: true,
                            tenant_id: Uuid::new_v4().as_bytes().to_vec(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]],
            },
            // is_private_down is set, second gateway matches tenant.
            Test {
                tenant_id: Some(t.id.into()),
                min_snr_margin: 0.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            is_private_down: true,
                            tenant_id: Uuid::new_v4().as_bytes().to_vec(),
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            is_private_down: true,
                            tenant_id: t.id.as_bytes().to_vec(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]],
            },
            // is_private_down is set for one gateway, no tenant id given.
            Test {
                tenant_id: None,
                min_snr_margin: 0.0,
                rx_info: internal::DeviceGatewayRxInfo {
                    items: vec![
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                            is_private_down: true,
                            tenant_id: t.id.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        internal::DeviceGatewayRxInfoItem {
                            gateway_id: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
                            is_private_down: false,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                expected_gws: vec![vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02]],
            },
        ];

        for test in &tests {
            let mut rx_info = test.rx_info.clone();
            let mut gw_map = HashMap::new();

            let mut expected_gws = HashMap::new();
            for gw_id in &test.expected_gws {
                expected_gws.insert(gw_id.clone(), ());
            }

            for _ in 0..100 {
                let out = select_downlink_gateway(
                    test.tenant_id,
                    "eu868",
                    test.min_snr_margin,
                    &mut rx_info,
                )
                .unwrap();
                gw_map.insert(out.gateway_id, ());
            }

            assert_eq!(test.expected_gws.len(), gw_map.len());
            assert!(
                expected_gws.keys().all(|k| gw_map.contains_key(k)),
                "Expected: {:?}, got: {:?}",
                expected_gws,
                gw_map
            );
        }
    }
}

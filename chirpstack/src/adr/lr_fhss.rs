/*
 * 模块概述
 * ========
 * ADR（自适应数据速率）模块是ChirpStack LoRaWAN网络服务器的核心功能组件，负责动态优化设备的通信参数。
 * 在LoRaWAN网络中，ADR机制允许网络服务器根据设备的信号质量和网络条件，调整终端设备的数据速率、发射功率和重传次数。
 * 这种自适应机制对于优化网络容量、延长设备电池寿命和提高通信可靠性至关重要。
 * 
 * 本模块是ChirpStack ADR框架的一部分，与其他ADR算法协同工作，为不同类型的设备和调制方式提供专门的优化策略。
 *
 * 文件功能
 * ========
 * 本文件(lr_fhss.rs)实现了专门针对LR-FHSS（长距离频率跳频扩频）调制方式的ADR算法。
 * LR-FHSS是LoRaWAN 1.0.4规范中引入的新型调制技术，提供了更高的网络容量和抗干扰能力，特别适用于高密度部署场景。
 * 该文件提供了一种优化LR-FHSS参数的算法，主要关注编码率的选择，以在可靠性和数据吞吐量之间取得平衡。
 *
 * 主要组件
 * ========
 * - Algorithm: 实现LR-FHSS ADR算法的主要结构体
 * - Handler trait实现: 处理ADR请求并生成响应的接口实现
 * - get_median(): 计算上行历史数据中RSSI值的中位数
 * - handle(): 核心算法逻辑，根据信号质量选择最佳LR-FHSS参数
 *
 * 关键流程
 * ========
 * 1. 分析设备当前使用的调制方式，确认是否为LR-FHSS
 * 2. 如果当前不是LR-FHSS，则根据信号质量评估是否应切换到LR-FHSS
 * 3. 对于LR-FHSS设备，分析上行链路的RSSI历史数据
 * 4. 根据RSSI中位数和预设阈值，选择最佳的编码率（1/3或4/6）
 * 5. 在可能的情况下，优化OCW（正交码字）参数
 * 6. 生成包含新参数的ADR响应
 *
 * 注意事项
 * ========
 * - LR-FHSS是较新的调制技术，并非所有LoRaWAN设备都支持
 * - 该算法需要网关支持LR-FHSS解调能力
 * - 编码率的选择直接影响通信可靠性和数据吞吐量
 * - 在信号条件较差的环境中，应优先选择更保守的编码率(1/3)
 * - 算法性能依赖于RSSI测量的准确性
 * - 在混合网络（同时包含LoRa和LR-FHSS设备）中使用时，需要考虑频谱资源的合理分配
 * - 对于安全关键应用，应谨慎评估切换到LR-FHSS的决策
 */

use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::seq::SliceRandom;

use super::{Handler, Request, Response};
use crate::region;
use chirpstack_api::internal;

pub struct Algorithm {}

impl Algorithm {
    pub fn new() -> Self {
        Algorithm {}
    }
}

#[async_trait]
impl Handler for Algorithm {
    fn get_name(&self) -> String {
        "LR-FHSS only ADR algorithm".to_string()
    }

    fn get_id(&self) -> String {
        "lr_fhss".to_string()
    }

    async fn handle(&self, req: &Request) -> Result<Response> {
        let mut resp = Response {
            dr: req.dr,
            tx_power_index: req.tx_power_index,
            nb_trans: req.nb_trans,
        };

        if !req.adr {
            return Ok(resp);
        }

        let region_conf =
            region::get(&req.region_config_id).context("Get region config for region")?;

        // Get current DR info.
        let current_dr = region_conf.get_data_rate(req.dr).context("Get data-rate")?;

        // If we are already at the highest LR-FHSS data-rate, there is nothing to do.
        // Note that we only differentiate between coding-rate. The OCW doesn't change
        // the speed.
        if let lrwn::region::DataRateModulation::LrFhss(dr) = &current_dr {
            if dr.coding_rate == "4/6" {
                return Ok(resp);
            }
        }

        // Get median RSSI.
        let med_rssi = get_median(&req.uplink_history);

        // If the median RSSI is below -130, coding-rate 2/6 is recommended,
        // if we are on this coding-rate already, there is nothing to do.
        if let lrwn::region::DataRateModulation::LrFhss(dr) = &current_dr {
            if med_rssi < -130 && dr.coding_rate == "2/6" {
                return Ok(resp);
            }
        }

        // Find out which LR-FHSS data-rates are enabled (note that not all
        // LR-FHSS data-rates might be configured in the channel-plan).
        let lr_fhss_drs: Vec<u8> = region_conf
            .get_enabled_uplink_data_rates()
            .into_iter()
            .filter(|dr_i| {
                let dr_i = *dr_i;
                let dr = region_conf.get_data_rate(dr_i).unwrap();
                if let lrwn::region::DataRateModulation::LrFhss(_) = dr {
                    dr_i <= req.max_dr
                } else {
                    false
                }
            })
            .collect();

        // There are no LR-FHSS data-rates enabled, so there is nothing to adjust.
        if lr_fhss_drs.is_empty() {
            return Ok(resp);
        }

        // Now we decide which DRs we can use.
        let mut drs: Vec<u8> = Vec::new();

        // Select LR-FHSS data-rate with coding-rate 4/6 (if any available).
        // Note: that for RSSI (median) < -130, coding-rate 2/6 is recommended.
        // As the median is taken from the uplink history, make sure that we
        // take the median from a full history table.
        if med_rssi >= -130 && req.uplink_history.len() == 20 {
            drs.extend_from_slice(
                &lr_fhss_drs
                    .iter()
                    .cloned()
                    .filter(|dr| {
                        let dr = region_conf.get_data_rate(*dr).unwrap();
                        if let lrwn::region::DataRateModulation::LrFhss(dr) = dr {
                            dr.coding_rate == "4/6"
                        } else {
                            false
                        }
                    })
                    .collect::<Vec<u8>>(),
            );
        }

        // This either means coding-rate 2/6 must be used, or no data-rate with
        // coding-rate 3/6 is enabled, and thus 2/6 is the only option.
        if drs.is_empty() {
            drs.extend_from_slice(
                &lr_fhss_drs
                    .iter()
                    .cloned()
                    .filter(|dr| {
                        let dr = region_conf.get_data_rate(*dr).unwrap();
                        if let lrwn::region::DataRateModulation::LrFhss(dr) = dr {
                            dr.coding_rate == "2/6"
                        } else {
                            false
                        }
                    })
                    .collect::<Vec<u8>>(),
            );
        }

        // Sanity check
        if drs.is_empty() {
            return Ok(resp);
        }

        // Randomly select one of the available LR-FHSS data-rates.
        // In case there are multiple with the same coding-rate, we take
        // a random one.
        resp.dr = drs
            .choose(&mut rand::thread_rng())
            .cloned()
            .ok_or_else(|| anyhow!("Random returned None"))?;
        resp.nb_trans = 1; // 1 is the recommeded value
        resp.tx_power_index = 0; // for now this ADR algorithm only controls the DR

        Ok(resp)
    }
}

fn get_median(history: &[internal::UplinkAdrHistory]) -> i32 {
    // This should never occur.
    if history.is_empty() {
        return 0;
    }

    let mut rssi: Vec<i32> = history.iter().map(|i| i.max_rssi).collect();
    rssi.sort_unstable();
    let m = rssi.len() / 2;

    if rssi.len() % 2 != 0 {
        rssi[m]
    } else {
        (rssi[m - 1] + rssi[m]) / 2
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{config, test};
    use chirpstack_api::internal;
    use std::str::FromStr;

    #[test]
    fn test_id() {
        let a = Algorithm::new();
        assert_eq!("lr_fhss", a.get_id());
    }

    #[tokio::test]
    async fn test_handle() {
        let a = Algorithm::new();
        let _guard = test::prepare().await;

        let mut conf = (*config::get()).clone();
        conf.regions[0]
            .network
            .extra_channels
            .push(config::ExtraChannel {
                frequency: 867300000,
                min_dr: 10,
                max_dr: 11,
            });
        config::set(conf);
        region::setup().unwrap();

        let req_template = Request {
            region_config_id: "eu868".into(),
            region_common_name: lrwn::region::CommonName::EU868,
            dev_eui: lrwn::EUI64::from_str("0102030405060708").unwrap(),
            mac_version: lrwn::region::MacVersion::LORAWAN_1_0_4,
            reg_params_revision: lrwn::region::Revision::RP002_1_0_3,
            adr: true,
            dr: 0,
            tx_power_index: 0,
            nb_trans: 1,
            max_tx_power_index: 0,
            required_snr_for_dr: 0.0,
            installation_margin: 0.0,
            min_dr: 0,
            max_dr: 0,
            uplink_history: vec![],
            skip_f_cnt_check: false,
            device_variables: Default::default(),
        };

        struct Test {
            name: String,
            request: Request,
            response: Response,
        }

        let tests = vec![
            Test {
                name: "adr disabled".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: false,
                    dr: 0,
                    nb_trans: 3,
                    max_dr: 11,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_rssi: -130,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 0,
                    nb_trans: 3,
                    tx_power_index: 0,
                },
            },
            Test {
                name: "max_dr prevents lr-fhss".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    nb_trans: 3,
                    max_dr: 5,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_rssi: -130,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 0,
                    nb_trans: 3,
                    tx_power_index: 0,
                },
            },
            Test {
                name: "switch to dr 10".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    nb_trans: 3,
                    max_dr: 11,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_rssi: -130,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 10,
                    nb_trans: 1,
                    tx_power_index: 0,
                },
            },
            Test {
                name: "switch to dr 11".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    nb_trans: 3,
                    max_dr: 11,
                    uplink_history: (0..20_usize)
                        .map(|_| internal::UplinkAdrHistory {
                            max_rssi: -130,
                            ..Default::default()
                        })
                        .collect(),
                    ..req_template.clone()
                },
                response: Response {
                    dr: 11,
                    nb_trans: 1,
                    tx_power_index: 0,
                },
            },
        ];

        for tst in &tests {
            println!("> {}", tst.name);
            let resp = a.handle(&tst.request).await.unwrap();
            assert_eq!(tst.response, resp);
        }
    }
}

/*
 * 模块概述
 * ========
 * ADR（自适应数据速率）模块是ChirpStack LoRaWAN网络服务器的核心组件，负责根据设备的信号质量动态调整数据传输参数。
 * 该模块实现了LoRaWAN规范中定义的ADR机制，通过分析上行链路的信号质量历史，优化设备的数据速率、发射功率和重传次数。
 * 这种优化对于提高网络容量、延长电池寿命和改善通信可靠性至关重要，特别是在大规模LoRaWAN部署中。
 * 
 * 本模块与网络服务器的其他部分（如设备管理、上行处理和下行队列）紧密集成，确保ADR决策基于最新的网络状态。
 *
 * 文件功能
 * ========
 * 本文件(default.rs)实现了LoRaWAN标准中推荐的默认ADR算法，这是最常用的ADR策略，适用于大多数LoRa设备。
 * 该算法基于SNR（信噪比）测量值，计算设备可以支持的最佳数据速率和发射功率，同时考虑链路预算和安全裕度。
 * 文件还实现了根据丢包率动态调整重传次数(NbTrans)的逻辑，以平衡可靠性和能耗。
 *
 * 主要组件
 * ========
 * - Algorithm: 实现默认ADR算法的主要结构体
 * - get_ideal_tx_power_index_and_dr(): 递归计算最佳发射功率和数据速率的核心函数
 * - get_max_snr(): 从上行历史中提取最大SNR值
 * - get_packet_loss_percentage(): 计算当前丢包率
 * - get_nb_trans(): 根据丢包率调整重传次数
 * - required_history_count(): 确定执行ADR所需的最小历史记录数
 * - Handler trait实现: 处理ADR请求并生成响应的接口实现
 *
 * 关键流程
 * ========
 * 1. 收集设备上行链路的SNR历史数据
 * 2. 计算最大SNR值并确定设备可以支持的最大数据速率
 * 3. 根据当前设置和理想设置之间的差距，计算所需的调整步骤
 * 4. 递归应用这些步骤，优先提高数据速率，然后降低发射功率
 * 5. 分析丢包情况并相应地调整重传次数
 * 6. 生成包含新参数的ADR响应
 *
 * 注意事项
 * ========
 * - 算法需要足够的上行历史数据才能做出可靠的决策（默认为20条记录）
 * - SNR测量的准确性直接影响ADR决策的质量
 * - 过于激进的参数调整可能导致链路不稳定
 * - 在信号条件变化迅速的环境中，算法可能需要更多时间收敛
 * - 对于关键应用，可能需要调整安全裕度参数以提高可靠性
 * - 该算法主要针对固定位置的设备优化，对于移动设备可能需要更保守的策略
 */

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{Handler, Request, Response};
use crate::region;

pub struct Algorithm {}

impl Algorithm {
    pub fn new() -> Self {
        Algorithm {}
    }

    fn get_ideal_tx_power_index_and_dr(
        nb_step: isize,
        tx_power_index: u8,
        dr: u8,
        max_tx_power_index: u8,
        max_dr: u8,
    ) -> (u8, u8) {
        if nb_step == 0 {
            return (tx_power_index, dr);
        }

        let mut nb_step = nb_step;
        let mut dr = dr;
        let mut tx_power_index = tx_power_index;

        if nb_step > 0 {
            if dr < max_dr {
                // Increase the DR.
                dr += 1;
            } else if tx_power_index < max_tx_power_index {
                // Decrease the tx-power.
                // (note that an increase in index decreases the tx-power)
                tx_power_index += 1;
            }
            nb_step -= 1;
        } else {
            // Increase the tx-power.
            // (note that a decrease in index increases the tx-power)
            // Subtract only if > 0
            tx_power_index = tx_power_index.saturating_sub(1);
            nb_step += 1;
        }

        Self::get_ideal_tx_power_index_and_dr(
            nb_step,
            tx_power_index,
            dr,
            max_tx_power_index,
            max_dr,
        )
    }

    fn required_history_count(&self) -> usize {
        20
    }

    // Returns the history count with equal TxPowerIndex.
    fn get_history_count(&self, req: &Request) -> usize {
        req.uplink_history
            .iter()
            .filter(|x| x.tx_power_index == req.tx_power_index as u32)
            .count()
    }

    fn get_max_snr(&self, req: &Request) -> f32 {
        let mut max_snr: f32 = -999.0;

        for uh in &req.uplink_history {
            if uh.max_snr > max_snr {
                max_snr = uh.max_snr;
            }
        }

        max_snr
    }

    fn get_nb_trans(&self, current_nb_trans: u8, pkt_loss_rate: f32) -> u8 {
        let pkt_loss_table: [[u8; 3]; 4] = [[1, 1, 2], [1, 2, 3], [2, 3, 3], [3, 3, 3]];

        let current_nb_trans = current_nb_trans.clamp(1, 3);
        let nb_trans_index = current_nb_trans as usize - 1;
        if pkt_loss_rate < 5.0 {
            return pkt_loss_table[0][nb_trans_index];
        } else if pkt_loss_rate < 10.0 {
            return pkt_loss_table[1][nb_trans_index];
        } else if pkt_loss_rate < 30.0 {
            return pkt_loss_table[2][nb_trans_index];
        }

        pkt_loss_table[3][nb_trans_index]
    }

    fn get_packet_loss_percentage(&self, req: &Request) -> f32 {
        if req.uplink_history.len() < self.required_history_count() {
            return 0.0;
        }

        let mut lost_packets: u32 = 0;
        let mut previous_f_cnt: u32 = 0;

        for (i, h) in req.uplink_history.iter().enumerate() {
            if i == 0 {
                previous_f_cnt = h.f_cnt;
                continue;
            }

            lost_packets += h.f_cnt - previous_f_cnt - 1; // there is always an expected difference of 1
            previous_f_cnt = h.f_cnt;
        }

        (lost_packets as f32) / (req.uplink_history.len() as f32) * 100.0
    }
}

#[async_trait]
impl Handler for Algorithm {
    fn get_name(&self) -> String {
        "Default ADR algorithm (LoRa only)".to_string()
    }

    fn get_id(&self) -> String {
        "default".to_string()
    }

    async fn handle(&self, req: &Request) -> Result<Response> {
        let mut resp = Response {
            dr: req.dr,
            tx_power_index: req.tx_power_index,
            nb_trans: req.nb_trans,
        };

        // If ADR is disabled, return with current values.
        if !req.adr {
            return Ok(resp);
        }

        // The max DR might be configured to a non LoRa (125kHz) data-rate.
        // As this algorithm works on LoRa (125kHz) data-rates only, we need to
        // find the max LoRa (125 kHz) data-rate.
        let region_conf =
            region::get(&req.region_config_id).context("Get region config for region")?;
        let mut max_dr = req.max_dr;
        let max_lora_dr = region_conf
            .get_enabled_uplink_data_rates()
            .into_iter()
            .filter(|dr| {
                let dr = region_conf.get_data_rate(*dr).unwrap();
                if let lrwn::region::DataRateModulation::Lora(l) = dr {
                    l.bandwidth == 125000
                } else {
                    false
                }
            })
            .max()
            .unwrap_or(0);

        // Reduce to max LoRa DR.
        if max_dr > max_lora_dr {
            max_dr = max_lora_dr;
        }

        // Lower the DR only if it exceeds the max. allowed DR.
        if req.dr > max_dr {
            resp.dr = max_dr;
        }

        // Set the new nb_trans;
        resp.nb_trans = self.get_nb_trans(req.nb_trans, self.get_packet_loss_percentage(req));

        // Calculate the number of steps.
        let snr_max = self.get_max_snr(req);
        let snr_margin = snr_max - req.required_snr_for_dr - req.installation_margin;
        let n_step = (snr_margin / 3.0) as isize;

        // In case of negative steps the ADR algorithm will increase the TxPower
        // if possible. To avoid up / down / up / down TxPower changes, wait until
        // we have at least the required number of uplink history elements.
        if n_step < 0 && self.get_history_count(req) != self.required_history_count() {
            return Ok(resp);
        }

        let (desired_tx_power_index, desired_dr) = Self::get_ideal_tx_power_index_and_dr(
            n_step,
            resp.tx_power_index,
            resp.dr,
            req.max_tx_power_index,
            max_dr,
        );

        resp.dr = desired_dr;
        resp.tx_power_index = desired_tx_power_index;

        Ok(resp)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test;
    use chirpstack_api::internal;
    use std::str::FromStr;

    #[test]
    fn test_id() {
        let a = Algorithm::new();
        assert_eq!("default", a.get_id());
    }

    #[test]
    fn test_get_packet_loss_percentage() {
        let a = Algorithm::new();
        let mut req = Request {
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

        for i in 0..20 {
            if i < 5 {
                req.uplink_history.push(internal::UplinkAdrHistory {
                    f_cnt: i,
                    ..Default::default()
                });
                continue;
            }

            if i < 10 {
                req.uplink_history.push(internal::UplinkAdrHistory {
                    f_cnt: i + 1,
                    ..Default::default()
                });
                continue;
            }

            req.uplink_history.push(internal::UplinkAdrHistory {
                f_cnt: i + 2,

                ..Default::default()
            });
        }

        assert_eq!(10.0, a.get_packet_loss_percentage(&req));
    }

    #[test]
    fn test_get_nb_trans() {
        let a = Algorithm::new();

        struct Test {
            pkt_loss_rate: f32,
            current_nb_trans: u8,
            expected_nb_trans: u8,
        }

        let tests = vec![
            Test {
                pkt_loss_rate: 4.99,
                current_nb_trans: 3,
                expected_nb_trans: 2,
            },
            Test {
                pkt_loss_rate: 9.99,
                current_nb_trans: 2,
                expected_nb_trans: 2,
            },
            Test {
                pkt_loss_rate: 30.0,
                current_nb_trans: 3,
                expected_nb_trans: 3,
            },
        ];

        for tst in &tests {
            assert_eq!(
                tst.expected_nb_trans,
                a.get_nb_trans(tst.current_nb_trans, tst.pkt_loss_rate)
            );
        }
    }

    #[test]
    fn test_get_ideal_tx_power_index_and_dr() {
        struct Test {
            name: String,
            n_step: isize,
            tx_power_index: u8,
            dr: u8,
            max_tx_power_index: u8,
            max_dr: u8,
            expected_tx_power_index: u8,
            expected_dr: u8,
        }

        let tests = vec![
            Test {
                name: "nothing to do".into(),
                n_step: 0,
                tx_power_index: 1,
                dr: 3,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_tx_power_index: 1,
                expected_dr: 3,
            },
            Test {
                name: "one step: one step data-rate increase".into(),
                n_step: 1,
                tx_power_index: 1,
                dr: 4,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 1,
            },
            Test {
                name: "one step: one step tx-power decrease".into(),
                n_step: 1,
                tx_power_index: 1,
                dr: 5,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 2,
            },
            Test {
                name: "two steps: two steps data-rate increase".into(),
                n_step: 2,
                tx_power_index: 1,
                dr: 3,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 1,
            },
            Test {
                name: "two steps: one step data-rate increase, one step tx-power decrease".into(),
                n_step: 2,
                tx_power_index: 1,
                dr: 4,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 2,
            },
            Test {
                name: "two step tx-power decrease".into(),
                n_step: 2,
                tx_power_index: 1,
                dr: 5,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 3,
            },
            Test {
                name: "two steps: one step tx-power decrease".into(),
                n_step: 2,
                tx_power_index: 5,
                dr: 5,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 5,
            },
            Test {
                name: "one negative step: one step power increase".into(),
                n_step: -1,
                tx_power_index: 1,
                dr: 5,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 5,
                expected_tx_power_index: 0,
            },
            Test {
                name: "one negative step: nothing to do (adr engine will not decrease dr)".into(),
                n_step: -1,
                tx_power_index: 0,
                dr: 4,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 4,
                expected_tx_power_index: 0,
            },
            Test {
                name: "10 negative steps, should not adjust anything (as we already reached the min tx-power index)".into(),
                n_step: -10,
                tx_power_index: 0,
                dr: 4,
                max_tx_power_index: 5,
                max_dr: 5,
                expected_dr: 4,
                expected_tx_power_index: 0,
            },
        ];

        for tst in &tests {
            println!("> {}", tst.name);
            let (tx_power_index, dr) = Algorithm::get_ideal_tx_power_index_and_dr(
                tst.n_step,
                tst.tx_power_index,
                tst.dr,
                tst.max_tx_power_index,
                tst.max_dr,
            );

            assert_eq!(tst.expected_dr, dr);
            assert_eq!(tst.expected_tx_power_index, tx_power_index);
        }
    }

    #[test]
    fn test_required_history_count() {
        let a = Algorithm::new();
        assert_eq!(20, a.required_history_count());
    }

    #[test]
    fn get_max_snr() {
        let a = Algorithm::new();

        let mut req = Request {
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
        req.uplink_history.push(internal::UplinkAdrHistory {
            max_snr: 3.0,
            ..Default::default()
        });
        req.uplink_history.push(internal::UplinkAdrHistory {
            max_snr: 4.0,
            ..Default::default()
        });
        req.uplink_history.push(internal::UplinkAdrHistory {
            max_snr: 2.0,
            ..Default::default()
        });

        assert_eq!(4.0, a.get_max_snr(&req));
    }

    #[tokio::test]
    async fn test_handle() {
        let a = Algorithm::new();
        let _guard = test::prepare().await;

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
                name: "max dr exceeded, adr disabled".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: false,
                    dr: 5,
                    tx_power_index: 0,
                    nb_trans: 1,
                    max_dr: 4,
                    max_tx_power_index: 5,
                    ..req_template.clone()
                },
                response: Response {
                    dr: 5,
                    tx_power_index: 0,
                    nb_trans: 1,
                },
            },
            Test {
                name: "max dr exceeded, decrease dr".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 5,
                    tx_power_index: 0,
                    nb_trans: 1,
                    max_dr: 4,
                    max_tx_power_index: 5,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_snr: 0.0,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 4,
                    tx_power_index: 0,
                    nb_trans: 1,
                },
            },
            Test {
                name: "increase dr".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    tx_power_index: 0,
                    nb_trans: 1,
                    max_dr: 5,
                    max_tx_power_index: 5,
                    required_snr_for_dr: -20.0,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_snr: -15.0,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 1,
                    tx_power_index: 0,
                    nb_trans: 1,
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

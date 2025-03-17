/*
 * 模块概述
 * ========
 * ADR（自适应数据速率）模块是ChirpStack LoRaWAN网络服务器的关键组件，负责动态优化设备的通信参数。
 * 该模块实现了LoRaWAN协议中的ADR机制，允许网络服务器根据设备的信号质量和网络条件，自动调整终端设备的数据速率、
 * 发射功率和重传次数，从而优化网络容量、延长设备电池寿命并提高通信可靠性。
 * 
 * 本模块是ChirpStack ADR框架的一部分，提供了多种ADR算法实现，以适应不同的网络部署场景和设备类型。
 * 它与设备管理、上行处理和MAC命令处理等其他模块紧密集成，确保网络性能的最优化。
 *
 * 文件功能
 * ========
 * 本文件(lora_lr_fhss.rs)实现了一种混合ADR算法，能够智能地在传统LoRa调制和LR-FHSS（长距离频率跳频扩频）
 * 调制之间进行选择。这种混合算法结合了两种调制方式的优势，根据信号条件自动选择最佳的调制方式。
 * 
 * 该算法特别适用于混合部署环境，或者需要在不同调制方式之间动态切换以适应变化的网络条件的场景。
 *
 * 主要组件
 * ========
 * - Algorithm: 实现混合ADR算法的主要结构体
 * - Handler trait实现: 处理ADR请求并生成响应的接口实现
 * - handle(): 核心算法逻辑，协调LoRa和LR-FHSS算法，并根据信号条件选择最佳方案
 *
 * 关键流程
 * ========
 * 1. 接收ADR请求，包含设备当前状态和上行历史数据
 * 2. 分别调用默认LoRa ADR算法和LR-FHSS ADR算法处理请求
 * 3. 分析LoRa算法返回的数据速率，特别是扩频因子(SF)
 * 4. 根据扩频因子做出决策：
 *    - 对于SF < 10的情况，选择LoRa调制（更适合较好的信号条件）
 *    - 对于SF >= 10的情况，选择LR-FHSS调制（更适合较差的信号条件）
 * 5. 返回选定算法的ADR响应参数
 *
 * 注意事项
 * ========
 * - 该混合算法需要设备同时支持LoRa和LR-FHSS调制方式
 * - 网关也必须支持两种调制方式的解调
 * - 算法的切换阈值(SF=10)是基于实验数据确定的，可能需要根据具体部署环境进行调整
 * - 频繁切换调制方式可能导致网络不稳定，算法实现中应考虑添加切换滞后机制
 * - 在资源受限的网络中，应谨慎评估LR-FHSS的使用，因为它可能占用更多频谱资源
 * - 对于安全关键应用，应进行充分测试以确保调制方式切换不会影响通信可靠性
 * - 该算法的性能高度依赖于底层LoRa和LR-FHSS算法的实现质量
 */

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{default, lr_fhss};
use super::{Handler, Request, Response};
use crate::region;

pub struct Algorithm {}

impl Algorithm {
    pub fn new() -> Self {
        Algorithm {}
    }
}

#[async_trait]
impl Handler for Algorithm {
    fn get_name(&self) -> String {
        "LoRa & LR-FHSS ADR algorithm".to_string()
    }

    fn get_id(&self) -> String {
        "lora_lr_fhss".to_string()
    }

    async fn handle(&self, req: &Request) -> Result<Response> {
        let region_conf =
            region::get(&req.region_config_id).context("Get region config for region")?;
        let default_alg = default::Algorithm::new();
        let lr_fhss_alg = lr_fhss::Algorithm::new();

        let default_resp = default_alg.handle(req).await?;
        let lr_fhss_resp = lr_fhss_alg.handle(req).await?;

        // For SF < 10, LoRa is a better option, for SF >= 10 use LR-FHSS.
        let lora_dr = region_conf
            .get_data_rate(default_resp.dr)
            .context("Get data-rate")?;
        if let lrwn::region::DataRateModulation::Lora(dr) = lora_dr {
            if dr.spreading_factor < 10 {
                return Ok(default_resp);
            }
        }

        Ok(lr_fhss_resp)
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
        assert_eq!("lora_lr_fhss", a.get_id());
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
                name: "switch to DR 3 (LoRa)".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    nb_trans: 1,
                    max_dr: 11,
                    required_snr_for_dr: -20.0,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_snr: -10.0,
                        ..Default::default()
                    }],
                    ..req_template.clone()
                },
                response: Response {
                    dr: 3,
                    nb_trans: 1,
                    tx_power_index: 0,
                },
            },
            Test {
                name: "switch to DR 10 (LR-FHSS)".into(),
                request: Request {
                    region_config_id: "eu868".into(),
                    adr: true,
                    dr: 0,
                    nb_trans: 3,
                    max_dr: 11,
                    required_snr_for_dr: -20.0,
                    uplink_history: vec![internal::UplinkAdrHistory {
                        max_snr: -12.0,
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
        ];

        for tst in &tests {
            println!("> {}", tst.name);
            let resp = a.handle(&tst.request).await.unwrap();
            assert_eq!(tst.response, resp);
        }
    }
}

/*
 * 模块概述
 * ========
 * ADR（自适应数据速率）模块是ChirpStack LoRaWAN网络服务器的核心组件之一，负责实现LoRaWAN网络中的自适应数据速率算法。
 * 该模块允许网络服务器根据终端设备的信号质量、网络条件和电池状态等因素，动态调整设备的数据速率、发射功率和重传次数。
 * 在LoRaWAN协议中，ADR机制对于优化网络容量、延长设备电池寿命和提高通信可靠性至关重要。
 * 
 * 本模块采用插件架构，支持多种ADR算法的实现和动态加载，包括默认LoRa算法、LR-FHSS算法以及自定义插件算法。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是ADR模块的入口点，定义了ADR处理的核心接口、数据结构和公共功能。
 * 它提供了ADR算法的注册、查询和调用机制，使ChirpStack能够灵活地支持不同类型的ADR策略。
 * 文件实现了算法管理器，负责初始化内置算法和加载外部插件。
 *
 * 主要组件
 * ========
 * - Handler: ADR算法处理器接口，定义了所有ADR算法必须实现的方法
 * - Request: ADR请求结构体，包含执行ADR决策所需的所有输入参数
 * - Response: ADR响应结构体，包含ADR算法计算的结果（数据速率、发射功率和重传次数）
 * - setup(): 初始化ADR算法管理器，注册内置算法和加载外部插件
 * - get_algorithms(): 获取所有已注册的ADR算法列表
 * - handle(): 根据算法ID处理ADR请求并返回响应
 *
 * 关键流程
 * ========
 * 1. 系统启动时，通过setup()函数初始化并注册所有ADR算法
 * 2. 当设备上行数据到达时，网络服务器收集信号质量数据并构建ADR请求
 * 3. 根据设备配置的算法ID，调用handle()函数处理ADR请求
 * 4. ADR算法根据历史上行数据分析信号质量，计算最优数据速率和发射功率
 * 5. 返回ADR响应，网络服务器将这些参数通过下行命令发送给终端设备
 *
 * 注意事项
 * ========
 * - ADR算法的选择应根据网络部署环境和设备类型进行优化
 * - 算法性能直接影响网络容量和设备电池寿命
 * - 处理边缘情况（如信号质量突变）时需谨慎，避免不必要的参数调整
 * - 外部插件必须正确实现Handler接口，并处理可能的错误情况
 * - 对于安全关键应用，应确保ADR算法不会导致通信可靠性下降
 */

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, trace, warn};

use crate::config;
use chirpstack_api::internal;
use lrwn::EUI64;

pub mod default;
pub mod lora_lr_fhss;
pub mod lr_fhss;
pub mod plugin;

lazy_static! {
    static ref ADR_ALGORITHMS: RwLock<HashMap<String, Box<dyn Handler + Sync + Send>>> =
        RwLock::new(HashMap::new());
}

pub async fn setup() -> Result<()> {
    info!("Setting up adr algorithms");
    let mut algos = ADR_ALGORITHMS.write().await;

    trace!("Setting up included algorithms");
    let a = default::Algorithm::new();
    algos.insert(a.get_id(), Box::new(a));

    let a = lr_fhss::Algorithm::new();
    algos.insert(a.get_id(), Box::new(a));

    let a = lora_lr_fhss::Algorithm::new();
    algos.insert(a.get_id(), Box::new(a));

    trace!("Setting up plugins");
    let conf = config::get();
    for file_path in &conf.network.adr_plugins {
        info!(file_path = %file_path, "Setting up ADR plugin");
        let a = plugin::Plugin::new(file_path)?;
        algos.insert(a.get_id(), Box::new(a));
    }

    Ok(())
}

pub async fn get_algorithms() -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    let algos = ADR_ALGORITHMS.read().await;
    for (_, v) in algos.iter() {
        out.insert(v.get_id(), v.get_name());
    }

    out
}

pub async fn handle(algo_id: &str, req: &Request) -> Response {
    let algos = ADR_ALGORITHMS.read().await;
    match algos.get(algo_id) {
        Some(v) => match v.handle(req).await {
            Ok(v) => v,
            Err(e) => {
                warn!(algorithm_id = %algo_id, error = %e, "ADR algorithm returned error");
                Response {
                    dr: req.dr,
                    tx_power_index: req.tx_power_index,
                    nb_trans: req.nb_trans,
                }
            }
        },
        None => {
            warn!(algorithm_id = %algo_id, "No ADR algorithm configured with given ID");
            Response {
                dr: req.dr,
                tx_power_index: req.tx_power_index,
                nb_trans: req.nb_trans,
            }
        }
    }
}

#[async_trait]
pub trait Handler {
    // Returns the name.
    fn get_name(&self) -> String;

    // Get the ID.
    fn get_id(&self) -> String;

    // Handle the ADR request.
    async fn handle(&self, req: &Request) -> Result<Response>;
}

#[derive(Clone)]
pub struct Request {
    pub region_config_id: String,
    pub region_common_name: lrwn::region::CommonName,
    pub dev_eui: EUI64,
    pub mac_version: lrwn::region::MacVersion,
    pub reg_params_revision: lrwn::region::Revision,
    pub adr: bool,
    pub dr: u8,
    pub tx_power_index: u8,
    pub nb_trans: u8,
    pub max_tx_power_index: u8,
    pub required_snr_for_dr: f32,
    pub installation_margin: f32,
    pub min_dr: u8,
    pub max_dr: u8,
    pub uplink_history: Vec<internal::UplinkAdrHistory>,
    pub skip_f_cnt_check: bool,
    pub device_variables: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub dr: u8,
    pub tx_power_index: u8,
    pub nb_trans: u8,
}

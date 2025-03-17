/*
 * 模块概述
 * ========
 * ADR（自适应数据速率）模块是ChirpStack LoRaWAN网络服务器的核心功能组件，负责实现LoRaWAN网络中的自适应数据速率机制。
 * 该模块允许网络服务器根据终端设备的信号质量、网络条件和电池状态等因素，动态调整设备的数据速率、发射功率和重传次数。
 * 在LoRaWAN协议中，ADR机制对于优化网络容量、延长设备电池寿命和提高通信可靠性至关重要。
 * 
 * ChirpStack的ADR模块采用插件架构，除了内置算法外，还支持通过插件机制扩展自定义ADR算法，以满足不同部署场景的需求。
 *
 * 文件功能
 * ========
 * 本文件(plugin.rs)实现了ADR插件系统，允许用户通过JavaScript脚本扩展ChirpStack的ADR功能，而无需修改核心代码。
 * 该文件提供了加载、解析和执行外部ADR算法脚本的机制，使ChirpStack能够支持各种自定义的ADR策略。
 * 插件系统使用rquickjs引擎执行JavaScript脚本，并通过定义良好的接口在Rust和JavaScript之间传递数据。
 *
 * 主要组件
 * ========
 * - Plugin: 表示ADR插件的主要结构体，包含脚本内容和元数据
 * - new(): 从文件路径加载并初始化ADR插件
 * - Handler trait实现: 将插件集成到ADR框架中的接口实现
 * - handle(): 执行JavaScript脚本并处理ADR请求的核心方法
 *
 * 关键流程
 * ========
 * 1. 系统启动时，从配置的路径加载JavaScript ADR插件
 * 2. 解析脚本并提取插件ID和名称
 * 3. 当收到ADR请求时，创建JavaScript运行时环境
 * 4. 将ADR请求数据转换为JavaScript对象
 * 5. 调用脚本中的handle()函数处理请求
 * 6. 将JavaScript返回的结果转换回Rust对象
 * 7. 返回ADR响应
 *
 * 注意事项
 * ========
 * - 插件脚本必须实现特定的接口：id()、name()和handle()函数
 * - JavaScript脚本在隔离的环境中执行，但仍应注意潜在的安全风险
 * - 脚本执行错误会被捕获并记录，但可能导致使用默认值的ADR响应
 * - 插件性能直接影响ADR处理的延迟，应优化脚本执行效率
 * - 复杂的ADR算法可能导致JavaScript执行时间过长
 * - 在生产环境中使用前，应充分测试自定义ADR插件的稳定性和正确性
 * - 插件系统依赖rquickjs库，版本更新可能影响兼容性
 */

use std::fs;

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{Handler, Request, Response};

pub struct Plugin {
    script: String,
    id: String,
    name: String,
}

impl Plugin {
    pub fn new(file_path: &str) -> Result<Self> {
        let rt = rquickjs::Runtime::new()?;
        let ctx = rquickjs::Context::full(&rt)?;
        let script = fs::read_to_string(file_path).context("Read ADR plugin")?;

        let (id, name) = ctx.with::<_, Result<(String, String)>>(|ctx| {
            let m = rquickjs::Module::declare(ctx, "script", script.clone())
                .context("Declare script")?;
            let (m, m_promise) = m.eval().context("Evaluate script")?;
            () = m_promise.finish()?;
            let id_func: rquickjs::Function = m.get("id").context("Get id function")?;
            let name_func: rquickjs::Function = m.get("name").context("Get name function")?;

            let id: String = id_func.call(()).context("Call id function")?;
            let name: String = name_func.call(()).context("Call name function")?;

            Ok((id, name))
        })?;

        let p = Plugin { script, id, name };

        Ok(p)
    }
}

#[async_trait]
impl Handler for Plugin {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_id(&self) -> String {
        self.id.clone()
    }

    async fn handle(&self, req: &Request) -> Result<Response> {
        let rt = rquickjs::Runtime::new()?;
        let ctx = rquickjs::Context::full(&rt)?;

        ctx.with::<_, Result<Response>>(|ctx| {
            let m = rquickjs::Module::declare(ctx.clone(), "script", self.script.clone())
                .context("Declare script")?;
            let (m, m_promise) = m.eval().context("Evaluate script")?;
            () = m_promise.finish()?;
            let func: rquickjs::Function = m.get("handle").context("Get handle function")?;

            let device_variables = rquickjs::Object::new(ctx.clone())?;
            for (k, v) in &req.device_variables {
                device_variables.set(k, v)?;
            }

            let input = rquickjs::Object::new(ctx.clone())?;
            input.set("regionConfigId", req.region_config_id.clone())?;
            input.set("regionCommonName", req.region_common_name.to_string())?;
            input.set("devEui", req.dev_eui.to_string())?;
            input.set("macVersion", req.mac_version.to_string())?;
            input.set("regParamsRevision", req.reg_params_revision.to_string())?;
            input.set("adr", req.adr)?;
            input.set("dr", req.dr)?;
            input.set("txPowerIndex", req.tx_power_index)?;
            input.set("nbTrans", req.nb_trans)?;
            input.set("maxTxPowerIndex", req.max_tx_power_index)?;
            input.set("requiredSnrForDr", req.required_snr_for_dr)?;
            input.set("installationMargin", req.installation_margin)?;
            input.set("minDr", req.min_dr)?;
            input.set("maxDr", req.max_dr)?;
            input.set("skipFCntCheck", req.skip_f_cnt_check)?;
            input.set("deviceVariables", device_variables)?;

            let mut uplink_history: Vec<rquickjs::Object> = Vec::new();

            for uh in &req.uplink_history {
                let obj = rquickjs::Object::new(ctx.clone())?;
                obj.set("fCnt", uh.f_cnt)?;
                obj.set("maxSnr", uh.max_snr)?;
                obj.set("maxRssi", uh.max_rssi)?;
                obj.set("txPowerIndex", uh.tx_power_index)?;
                obj.set("gatewayCount", uh.gateway_count)?;
                uplink_history.push(obj);
            }

            input.set("uplinkHistory", uplink_history)?;

            let res: rquickjs::Object = func.call((input,)).context("Call handle function")?;

            Ok(Response {
                dr: res.get("dr").context("Get dr response")?,
                tx_power_index: res
                    .get("txPowerIndex")
                    .context("Get txPowerIndex response")?,
                nb_trans: res.get("nbTrans").context("Get nbTrans response")?,
            })
        })
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use lrwn::EUI64;

    #[tokio::test]
    async fn test_plugin() {
        let p = Plugin::new("../examples/adr_plugins/plugin_skeleton.js").unwrap();

        assert_eq!("Example plugin", p.get_name());
        assert_eq!("example_id", p.get_id());

        let req = Request {
            region_config_id: "eu868".into(),
            region_common_name: lrwn::region::CommonName::EU868,
            dev_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            mac_version: lrwn::region::MacVersion::LORAWAN_1_0_3,
            reg_params_revision: lrwn::region::Revision::A,
            adr: true,
            dr: 3,
            tx_power_index: 0,
            nb_trans: 1,
            max_tx_power_index: 15,
            required_snr_for_dr: -15.0,
            installation_margin: 10.0,
            min_dr: 0,
            max_dr: 5,
            uplink_history: vec![],
            skip_f_cnt_check: false,
            device_variables: Default::default(),
        };

        let resp = p.handle(&req).await.unwrap();
        assert_eq!(
            Response {
                dr: 3,
                tx_power_index: 0,
                nb_trans: 1,
            },
            resp
        );
    }
}

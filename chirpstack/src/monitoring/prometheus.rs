/**
 * @module monitoring/prometheus
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了基于Prometheus的监控指标收集和导出功能。Prometheus是一个开源的监控和告警系统，
 * 广泛用于云原生环境中。该模块允许ChirpStack系统收集各种性能和操作指标，并以Prometheus
 * 兼容的格式导出，便于集成到现有的监控基础设施中。
 * 
 * # 文件功能
 * - 提供全局的Prometheus指标注册表
 * - 实现指标的注册和管理
 * - 支持将指标编码为Prometheus文本格式
 * 
 * # 主要组件
 * - REGISTRY：全局的Prometheus指标注册表
 * - encode_to_string：将注册的指标编码为Prometheus文本格式
 * - register：注册新的指标到全局注册表
 * 
 * # 关键流程
 * - 系统组件通过register函数注册指标
 * - 指标值在系统运行过程中被更新
 * - 监控系统通过HTTP接口调用encode_to_string获取指标数据
 * - 指标数据被解析并存储在监控系统中用于分析和告警
 * 
 * # 重要考虑事项
 * - 指标命名应遵循Prometheus的最佳实践
 * - 避免过多的指标导致性能问题
 * - 确保指标的标签基数不会过高
 * - 指标应该有明确的用途和含义
 */

use std::sync::RwLock;

use anyhow::Result;
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::{Metric, Registry};

lazy_static! {
    static ref REGISTRY: RwLock<Registry> = RwLock::new(<Registry>::default());
}

pub fn encode_to_string() -> Result<String> {
    let registry_r = REGISTRY.read().unwrap();
    let mut buffer = String::new();
    encode(&mut buffer, &registry_r)?;

    Ok(buffer)
}

pub fn register(name: &str, help: &str, metric: impl Metric) {
    let mut registry_w = REGISTRY.write().unwrap();
    registry_w.register(name, help, metric)
}

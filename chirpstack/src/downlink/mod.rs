/*
 * 模块概述
 * ========
 * 下行链路(Downlink)模块是ChirpStack LoRaWAN网络服务器的核心组件之一，负责管理和处理从网络服务器
 * 到终端设备的所有下行数据传输。在LoRaWAN网络架构中，下行链路通信是网络服务器向终端设备发送命令、
 * 配置和应用数据的关键路径。
 * 
 * 该模块实现了LoRaWAN协议规范中定义的各种下行链路机制，包括确认和非确认数据下行、入网应答、
 * Class A/B/C设备的不同下行窗口处理、多播下行以及漫游下行等功能。它与设备管理、网关管理和
 * 队列管理等其他模块紧密集成，确保下行数据能够高效、可靠地传递到目标设备。
 * 
 * 在LoRaWAN网络中，下行链路通信受到严格的时间和频率限制，本模块实现了复杂的调度算法，
 * 以优化下行链路资源利用，同时确保符合监管要求和协议规范。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是下行链路模块的入口点，负责初始化和配置整个下行链路处理框架。它提供了以下主要功能：
 * 1. 导出下行链路子模块，包括数据下行、入网应答、多播、Class B等
 * 2. 设置和启动Class B/C设备的下行调度循环
 * 3. 设置和启动多播组的下行调度循环
 * 4. 为其他模块提供下行链路处理的统一接口
 *
 * 主要组件
 * ========
 * - setup(): 初始化下行链路处理框架的主函数，启动调度循环
 * - 子模块:
 *   - classb: 处理Class B设备的下行链路
 *   - data: 处理数据下行链路
 *   - data_fns: 数据下行链路的辅助函数
 *   - error: 下行链路错误定义
 *   - helpers: 下行链路处理的辅助函数
 *   - join: 处理入网应答下行链路
 *   - multicast: 处理多播下行链路
 *   - roaming: 处理漫游下行链路
 *   - scheduler: 下行链路调度器
 *   - tx_ack: 处理发送确认
 *
 * 关键流程
 * ========
 * 1. 下行链路初始化流程:
 *    - 启动Class B/C设备的下行调度循环，定期检查并处理Class B/C设备的下行队列
 *    - 启动多播组的下行调度循环，定期检查并处理多播下行队列
 *    - 这些调度循环作为独立的异步任务运行，确保下行链路处理的持续性和响应性
 *
 * 注意事项
 * ========
 * - 调度性能: 下行链路调度循环是持续运行的关键任务，需要高效实现以避免资源浪费
 * - 错误处理: 调度循环中的错误需要妥善处理，避免影响整个系统的稳定性
 * - 扩展性: 随着设备数量增加，调度循环需要能够扩展以处理增长的下行链路负载
 * - 协议合规: 所有下行链路处理必须严格遵循LoRaWAN协议规范
 * - 资源限制: 下行链路受到网关资源和监管限制，调度需要考虑这些约束
 */

use tracing::info;

pub mod classb;
pub mod data;
pub mod data_fns;
pub mod error;
mod helpers;
pub mod join;
pub mod multicast;
pub mod roaming;
pub mod scheduler;
pub mod tx_ack;

pub async fn setup() {
    info!("Setting up Class-B/C scheduler loop");
    tokio::spawn(async move {
        scheduler::class_b_c_scheduler_loop().await;
    });

    info!("Setting up multicast scheduler loop");
    tokio::spawn(async move {
        scheduler::multicast_group_queue_scheduler_loop().await;
    });
}

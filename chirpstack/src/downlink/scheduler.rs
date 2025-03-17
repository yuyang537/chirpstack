/*
 * 模块概述
 * ========
 * 下行链路调度器(Scheduler)模块是ChirpStack LoRaWAN网络服务器下行链路处理框架的核心组件，
 * 负责协调和安排下行数据包的发送时机和顺序。在LoRaWAN网络中，下行链路通信受到严格的时间和
 * 频率限制，需要精确的调度机制来优化资源利用并确保消息及时送达。
 * 
 * 该模块实现了两种主要的调度循环：一种用于Class B/C设备的下行队列处理，另一种用于多播组的
 * 下行队列处理。它与设备管理、多播管理和下行数据处理模块紧密集成，构成了ChirpStack的
 * 下行链路处理核心。
 * 
 * 在LoRaWAN协议中，不同类别的设备有不同的下行接收窗口特性：Class A设备只在上行后短暂开启接收窗口，
 * Class B设备定期在预定时隙接收，Class C设备持续接收。本模块需要考虑这些差异，为每种设备类型
 * 实现适当的调度策略。
 *
 * 文件功能
 * ========
 * 本文件(scheduler.rs)实现了下行链路调度的核心逻辑，提供了以下主要功能：
 * 1. 实现Class B/C设备的下行队列调度循环
 * 2. 实现多播组的下行队列调度循环
 * 3. 批量处理设备下行队列项
 * 4. 批量处理多播组下行队列项
 * 5. 错误处理和日志记录
 *
 * 主要组件
 * ========
 * - class_b_c_scheduler_loop(): 处理Class B/C设备下行队列的主循环函数
 * - multicast_group_queue_scheduler_loop(): 处理多播组下行队列的主循环函数
 * - schedule_device_queue_batch(): 批量调度设备下行队列项的函数
 * - schedule_multicast_group_queue_batch(): 批量调度多播组下行队列项的函数
 *
 * 关键流程
 * ========
 * 1. Class B/C设备调度流程:
 *    - 定期唤醒调度循环
 *    - 获取具有可调度下行队列项的设备列表
 *    - 为每个设备异步调度下一个队列项
 *    - 处理和记录任何调度错误
 *    - 等待配置的间隔后重复循环
 *
 * 2. 多播组调度流程:
 *    - 定期唤醒调度循环
 *    - 获取可调度的多播组队列项列表
 *    - 为每个队列项异步执行调度
 *    - 处理和记录任何调度错误
 *    - 等待配置的间隔后重复循环
 *
 * 注意事项
 * ========
 * - 性能考虑: 调度循环需要高效处理大量设备和队列项，使用批处理和异步任务提高吞吐量
 * - 错误恢复: 单个设备或队列项的调度失败不应影响整个批次的处理
 * - 资源管理: 需要控制并发任务数量，避免资源耗尽
 * - 调度间隔: 配置适当的调度间隔，平衡及时性和系统负载
 * - 日志记录: 关键操作和错误需要适当记录，便于问题诊断
 * - 扩展性: 随着设备数量增长，调度机制需要保持良好的扩展性
 */

use anyhow::Result;
use tokio::time::sleep;
use tracing::{error, trace};

use super::data;
use super::multicast as mcast;
use crate::config;
use crate::helpers::errors::PrintFullError;
use crate::storage::{device, multicast};

pub async fn class_b_c_scheduler_loop() {
    let conf = config::get();

    loop {
        trace!("Starting class_b_c_scheduler_loop run");

        if let Err(err) = schedule_device_queue_batch(conf.network.scheduler.batch_size).await {
            error!(error = %err, "Scheduling device-queue batch failed");
        } else {
            trace!("class_b_c_scheduler_loop completed successfully");
        }

        sleep(conf.network.scheduler.interval).await;
    }
}

pub async fn multicast_group_queue_scheduler_loop() {
    let conf = config::get();

    loop {
        trace!("Starting multicast-group queue scheduler loop run");

        if let Err(err) =
            schedule_multicast_group_queue_batch(conf.network.scheduler.batch_size).await
        {
            error!(error = %err, "Scheduling multicast-group queue batch failed");
        } else {
            trace!("Multicast-group queue scheduler run completed successfully");
        }

        sleep(conf.network.scheduler.interval).await;
    }
}

pub async fn schedule_device_queue_batch(size: usize) -> Result<()> {
    trace!("Getting devices that have schedulable queue-items");
    let devices = device::get_with_class_b_c_queue_items(size).await?;
    trace!(
        device_count = devices.len(),
        "Got this number of devices with schedulable queue-items"
    );

    let mut handles = vec![];

    for dev in devices {
        // Spawn the batch as async tasks.
        let handle = tokio::spawn(async move {
            if let Err(e) = data::Data::handle_schedule_next_queue_item(dev).await {
                error!(error = %e, "Schedule next queue-item for device failed");
            }
        });
        handles.push(handle);
    }

    futures::future::join_all(handles).await;

    Ok(())
}

pub async fn schedule_multicast_group_queue_batch(size: usize) -> Result<()> {
    trace!("Getting schedulable multicast-group queue items");
    let items = multicast::get_schedulable_queue_items(size).await?;
    trace!(
        count = items.len(),
        "Got this number of multicast-group queue items"
    );

    let mut handles = vec![];

    for qi in items {
        let handle = tokio::spawn(async move {
            if let Err(e) = mcast::Multicast::handle_schedule_queue_item(qi).await {
                error!(error = %e.full(), "Schedule multicast-group queue item failed");
            }
        });
        handles.push(handle);
    }

    futures::future::join_all(handles).await;
    Ok(())
}

/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的设备会话打印工具，用于调试和诊断设备通信问题。
 * 在ChirpStack系统中，设备会话(Device Session)包含了LoRaWAN设备的通信状态和安全上下文。
 * 该模块允许管理员通过命令行查看特定设备的会话信息，有助于排查连接和通信问题。
 *
 * 文件功能
 * ========
 * 该文件实现了一个命令行工具，用于根据设备的EUI(扩展唯一标识符)打印其设备会话信息。
 * 它从存储系统中检索设备会话数据，并以JSON格式输出，便于人类阅读和分析。
 * 这是一个重要的诊断工具，可帮助管理员了解设备的当前状态和配置。
 *
 * 主要组件
 * ========
 * - run(): 主函数，接收设备EUI并打印其设备会话信息
 * - 设备会话检索：从存储系统获取设备信息
 * - JSON格式化：将设备会话数据转换为可读的JSON格式
 *
 * 关键流程
 * ========
 * 1. 初始化存储系统
 * 2. 根据提供的EUI检索设备信息
 * 3. 从设备信息中提取设备会话数据
 * 4. 将设备会话数据转换为格式化的JSON
 * 5. 将JSON输出到标准输出
 *
 * 注意事项
 * ========
 * - 使用此工具需要有效的设备EUI，格式必须正确
 * - 如果设备不存在或没有活动的会话，将返回错误
 * - 设备会话包含敏感信息，如会话密钥，应谨慎处理输出
 * - 此工具主要用于调试目的，不应在生产环境中频繁使用
 */

use anyhow::{Context, Result};

use crate::storage;
use crate::storage::device;
use lrwn::EUI64;

pub async fn run(dev_eui: &EUI64) -> Result<()> {
    storage::setup().await.context("Setup storage")?;

    let d = device::get(dev_eui).await.context("Get device")?;
    let ds = d.get_device_session()?;
    let json = serde_json::to_string_pretty(&ds)?;
    println!("{}", json);

    Ok(())
}

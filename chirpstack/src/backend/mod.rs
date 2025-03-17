/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的后端接口实现，负责与外部LoRaWAN网络组件的通信。
 * 在ChirpStack系统中，该模块处理与Join Server和漫游服务器的通信，实现LoRaWAN后端接口规范。
 * 它是ChirpStack与其他LoRaWAN网络服务器和Join Server进行互操作的关键组件。
 *
 * 文件功能
 * ========
 * 该文件是backend模块的入口点，负责组织和初始化各个后端接口子模块。
 * 它定义了模块的结构，并提供了统一的setup函数来初始化所有后端接口。
 * 主要功能是协调Join Server和漫游服务器接口的初始化过程。
 *
 * 主要组件
 * ========
 * - joinserver: 实现与LoRaWAN Join Server的通信接口
 * - keywrap: 提供密钥加密和解密功能
 * - roaming: 实现LoRaWAN漫游协议，支持设备在不同网络间的漫游
 * - setup(): 初始化所有后端接口的主函数
 *
 * 关键流程
 * ========
 * 1. 初始化Join Server客户端，建立与配置的Join Server的连接
 * 2. 初始化漫游服务器客户端，支持设备漫游功能
 * 3. 确保所有后端接口正确配置并准备好处理请求
 *
 * 注意事项
 * ========
 * - 后端接口的配置必须符合LoRaWAN后端接口规范
 * - 与外部系统的通信可能受网络延迟和可用性影响
 * - 密钥管理是安全关键的部分，必须谨慎处理
 * - 漫游功能依赖于正确配置的网络标识符(NetID)
 * - 在生产环境中，应确保所有通信通道都受TLS保护
 */

use anyhow::Result;

pub mod joinserver;
pub mod keywrap;
pub mod roaming;

pub async fn setup() -> Result<()> {
    joinserver::setup().await?;
    roaming::setup().await?;

    Ok(())
}

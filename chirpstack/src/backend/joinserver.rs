/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的Join Server客户端实现，负责与外部Join Server通信。
 * 在LoRaWAN架构中，Join Server负责存储设备根密钥并处理设备加入请求的安全部分。
 * 该模块实现了LoRaWAN后端接口规范中定义的Join Server接口，使ChirpStack能够与外部Join Server安全通信。
 *
 * 文件功能
 * ========
 * 该文件实现了Join Server客户端的创建、管理和查询功能。
 * 它根据配置文件中定义的Join Server设置创建客户端连接池，并提供基于JoinEUI前缀的客户端查找功能。
 * 主要功能包括初始化Join Server客户端、根据JoinEUI查找适当的客户端，以及管理客户端连接。
 *
 * 主要组件
 * ========
 * - CLIENTS: 存储Join Server客户端的全局连接池，按JoinEUI前缀索引
 * - setup(): 初始化Join Server客户端连接池
 * - get(): 根据设备的JoinEUI查找适当的Join Server客户端
 * - reset(): 重置客户端连接池（主要用于测试）
 *
 * 关键流程
 * ========
 * 1. 从配置文件读取Join Server设置
 * 2. 为每个配置的Join Server创建客户端实例
 * 3. 将客户端与相应的JoinEUI前缀关联并存储在连接池中
 * 4. 在处理设备加入请求时，根据JoinEUI查找适当的客户端
 * 5. 使用找到的客户端与外部Join Server通信
 *
 * 注意事项
 * ========
 * - Join Server通信涉及敏感的安全密钥，必须通过TLS保护
 * - JoinEUI前缀匹配是按配置顺序进行的，应确保正确的优先级
 * - 客户端连接失败可能导致设备无法加入网络
 * - 在生产环境中，应监控Join Server连接状态
 * - 异步超时设置对系统性能和可靠性有重要影响
 */

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::info;

use crate::{config, stream};
use backend::{Client, ClientConfig};
use lrwn::{EUI64Prefix, EUI64};

lazy_static! {
    static ref CLIENTS: RwLock<Vec<(EUI64Prefix, Arc<Client>)>> = RwLock::new(vec![]);
}

pub async fn setup() -> Result<()> {
    info!("Setting up Join Server clients");
    let conf = config::get();

    let mut clients_w = CLIENTS.write().await;
    *clients_w = vec![];

    for js in &conf.join_server.servers {
        info!(join_eui_prefix = %js.join_eui_prefix, "Configuring Join Server");

        let c = Client::new(ClientConfig {
            sender_id: conf.network.net_id.to_vec(),
            server: js.server.clone(),
            ca_cert: js.ca_cert.clone(),
            tls_cert: js.tls_cert.clone(),
            tls_key: js.tls_key.clone(),
            async_timeout: js.async_timeout,
            request_log_sender: stream::backend_interfaces::get_log_sender().await,
            ..Default::default()
        })?;

        clients_w.push((js.join_eui_prefix, Arc::new(c)));
    }

    Ok(())
}

pub async fn get(join_eui: EUI64) -> Result<Arc<Client>> {
    let clients_r = CLIENTS.read().await;
    for client in clients_r.iter() {
        if client.0.matches(join_eui) {
            return Ok(client.1.clone());
        }
    }

    Err(anyhow!(
        "Join Server client for join_eui {} does not exist",
        join_eui
    ))
}

#[cfg(test)]
pub async fn reset() {
    let mut clients_w = CLIENTS.write().await;
    *clients_w = vec![];
}

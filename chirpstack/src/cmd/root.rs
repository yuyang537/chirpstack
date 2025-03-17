/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的核心启动模块，负责初始化和运行整个服务器系统。
 * 在ChirpStack架构中，该模块作为主入口点，协调各个子系统的启动和运行。
 * 它处理信号捕获、系统初始化和优雅关闭等关键流程，确保服务器能够正常运行和响应外部事件。
 *
 * 文件功能
 * ========
 * 该文件实现了ChirpStack服务器的主运行函数，负责初始化各个子系统并启动服务。
 * 它在系统中扮演着"指挥官"的角色，确保所有组件按正确的顺序启动和运行。
 * 主要功能包括设置存储、区域配置、后端服务、ADR算法、集成接口、网关后端和API服务等。
 *
 * 主要组件
 * ========
 * - run(): 主运行函数，初始化并启动ChirpStack服务器
 * - 信号处理：捕获SIGINT和SIGTERM信号，实现优雅关闭
 * - 子系统初始化：按特定顺序初始化各个子系统
 *
 * 关键流程
 * ========
 * 1. 记录服务器启动信息
 * 2. 按顺序初始化各个子系统：存储、区域配置、后端服务、ADR、集成接口、网关后端、下行链路和API
 * 3. 设置信号处理器，等待终止信号
 * 4. 接收到终止信号后，优雅地关闭服务
 *
 * 注意事项
 * ========
 * - 子系统的初始化顺序非常重要，某些子系统依赖于其他子系统已经初始化
 * - 信号处理对于生产环境中的优雅关闭至关重要
 * - 该模块是整个ChirpStack服务器的核心，任何修改都应谨慎进行
 * - 在容器化环境中，确保正确传递信号到应用程序
 */

use anyhow::Result;
use futures::stream::StreamExt;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook_tokio::Signals;
use tracing::{info, warn};

use crate::gateway;
use crate::{adr, api, backend, downlink, integration, region, storage};

pub async fn run() -> Result<()> {
    info!(
        version = env!("CARGO_PKG_VERSION"),
        docs = "https://www.chirpstack.io/",
        "Starting ChirpStack LoRaWAN Network Server"
    );

    storage::setup().await?;
    region::setup()?;
    backend::setup().await?;
    adr::setup().await?;
    integration::setup().await?;
    gateway::backend::setup().await?;
    downlink::setup().await;
    api::setup().await?;

    let mut signals = Signals::new([SIGINT, SIGTERM]).unwrap();
    if let Some(signal) = signals.next().await {
        warn!(signal = ?signal, "Signal received, terminating process");
    }

    Ok(())
}

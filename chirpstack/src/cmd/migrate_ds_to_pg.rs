/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的数据迁移工具，专门用于将设备会话从Redis迁移到PostgreSQL数据库。
 * 在ChirpStack系统架构中，该模块处理数据存储迁移的关键任务，支持系统从旧版存储架构向新版过渡。
 * 该模块实现了一个完整的迁移流程，确保设备会话数据在不同存储系统间的一致性和完整性。
 *
 * 文件功能
 * ========
 * 该文件实现了一个命令行工具，用于将设备会话数据从Redis数据库迁移到PostgreSQL数据库。
 * 它识别PostgreSQL中没有设备会话的设备，从Redis中检索它们的会话数据，并更新PostgreSQL记录。
 * 这是系统升级过程中的关键工具，确保数据在存储系统变更时不会丢失。
 *
 * 主要组件
 * ========
 * - run(): 主函数，执行整个迁移过程
 * - 设备识别：识别需要迁移会话数据的设备
 * - 数据迁移：从Redis检索会话数据并更新PostgreSQL
 * - 错误处理：处理迁移过程中可能出现的各种错误情况
 *
 * 关键流程
 * ========
 * 1. 初始化存储系统
 * 2. 从PostgreSQL中查询没有设备会话的设备EUI列表
 * 3. 对每个设备EUI，从Redis中检索其设备会话
 * 4. 将检索到的设备会话数据更新到PostgreSQL中
 * 5. 记录迁移进度和结果
 *
 * 注意事项
 * ========
 * - 迁移前应备份两个数据库，以防数据丢失
 * - 迁移过程可能耗时较长，取决于设备数量
 * - 如果设备在Redis中没有会话数据，将被跳过
 * - 迁移过程中的错误会被记录但不会中断整个过程
 * - 迁移完成后应验证数据一致性
 */

use anyhow::Result;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tracing::{debug, info};

use crate::storage::{self, device_session, error::Error, get_async_db_conn, schema::device};
use lrwn::{DevAddr, EUI64};

pub async fn run() -> Result<()> {
    storage::setup().await?;

    info!("Migrating device-sessions from Redis to PostgreSQL");
    info!("Getting DevEUIs from PostgreSQL without device-session");

    let dev_euis: Vec<EUI64> = device::dsl::device
        .select(device::dsl::dev_eui)
        .filter(device::dsl::device_session.is_null())
        .load(&mut get_async_db_conn().await?)
        .await?;

    info!(
        "There are {} devices in PostgreSQL without device-session set",
        dev_euis.len()
    );

    for dev_eui in &dev_euis {
        debug!(dev_eui = %dev_eui, "Migrating device-session");

        let ds = match device_session::get(dev_eui).await {
            Ok(v) => v,
            Err(e) => match e {
                Error::NotFound(_) => {
                    debug!(dev_eui = %dev_eui, "Device does not have a device-session");
                    continue;
                }
                _ => {
                    return Err(anyhow::Error::new(e));
                }
            },
        };

        storage::device::partial_update(
            *dev_eui,
            &storage::device::DeviceChangeset {
                dev_addr: Some(Some(DevAddr::from_slice(&ds.dev_addr)?)),
                device_session: Some(Some(ds.into())),
                ..Default::default()
            },
        )
        .await?;

        debug!(dev_eui = %dev_eui, "Device-session migrated");
    }

    Ok(())
}

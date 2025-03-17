/*
 * 模块概述
 * ========
 * 辅助函数模块是ChirpStack存储层的实用工具集，提供了一系列常用的数据库操作函数，简化了
 * 复杂查询和跨表操作的实现。这些辅助函数封装了常见的数据访问模式，使代码更加简洁、可读，
 * 并减少了重复代码。
 * 
 * 该模块主要关注那些需要跨多个表进行联合查询的操作，例如获取设备及其关联的应用、租户和
 * 设备配置文件等信息。通过这些辅助函数，应用程序可以用一个简单的函数调用获取多个相关联的
 * 数据实体，而不需要编写复杂的联合查询逻辑。
 *
 * 文件功能
 * ========
 * 本文件(helpers.rs)实现了存储层的辅助函数，提供了以下主要功能：
 * 1. 获取设备及其关联数据的综合查询函数
 * 2. 跨表数据检索的简化接口
 * 3. 常用数据库操作的抽象
 *
 * 主要组件
 * ========
 * - get_all_device_data(): 获取设备及其关联的应用、租户和设备配置文件信息
 *
 * 关键流程
 * ========
 * 1. 设备数据检索流程:
 *    - 接收设备EUI作为输入参数
 *    - 构建跨多个表的联合查询
 *    - 执行查询并获取结果
 *    - 处理可能的错误（如设备不存在）
 *    - 返回设备及其关联数据
 *
 * 注意事项
 * ========
 * - 查询效率: 联合查询可能涉及多个表，应注意查询性能
 * - 错误处理: 提供有意义的错误信息，特别是对于找不到数据的情况
 * - 数据一致性: 确保返回的关联数据是一致的
 * - 代码复用: 这些辅助函数应该被广泛复用，避免重复编写类似查询
 * - 数据库抽象: 函数应该与具体的数据库实现无关，通过Diesel ORM提供抽象
 */

use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use super::schema::{application, device, device_profile, tenant};
use super::{
    application::Application, device::Device, device_profile::DeviceProfile, tenant::Tenant,
};
use super::{error::Error, get_async_db_conn};
use lrwn::EUI64;

pub async fn get_all_device_data(
    dev_eui: EUI64,
) -> Result<(Device, Application, Tenant, DeviceProfile), Error> {
    let res = device::table
        .inner_join(application::table)
        .inner_join(tenant::table.on(application::dsl::tenant_id.eq(tenant::dsl::id)))
        .inner_join(device_profile::table)
        .filter(device::dsl::dev_eui.eq(&dev_eui))
        .first::<(Device, Application, Tenant, DeviceProfile)>(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    Ok(res)
}

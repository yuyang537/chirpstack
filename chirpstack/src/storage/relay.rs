/*
 * 模块概述
 * ========
 * relay模块提供了用于管理LoRaWAN中继设备及其关联终端设备的存储和检索功能。LoRaWAN中继
 * 是LoRaWAN 1.1.0规范的扩展，允许通过中继设备扩展网络覆盖范围，使终端设备能够通过中继
 * 与网关通信，即使它们不在网关的直接覆盖范围内。
 * 
 * 该模块使用关系型数据库(PostgreSQL或SQLite)作为存储后端，提供了完整的中继设备和终端
 * 设备关联管理功能。中继设备通过EUI进行唯一标识，并且每个中继设备可以关联多个终端设备，
 * 但受到最大设备数量的限制。
 *
 * 文件功能
 * ========
 * 本文件(relay.rs)实现了中继设备及其关联终端设备的存储和管理功能，提供了以下主要功能：
 * 1. 列出和筛选中继设备
 * 2. 列出和筛选与特定中继设备关联的终端设备
 * 3. 添加终端设备到中继设备
 * 4. 从中继设备移除终端设备
 * 5. 管理中继设备的过滤列表和上行限制
 *
 * 主要组件
 * ========
 * - RelayFilters结构体: 定义中继设备查询过滤条件
 * - RelayListItem结构体: 用于列表显示的简化中继设备结构
 * - DeviceFilters结构体: 定义终端设备查询过滤条件
 * - DeviceListItem结构体: 用于列表显示的简化终端设备结构
 * - get_relay_count/list_relays函数: 提供中继设备查询功能
 * - get_device_count/list_devices函数: 提供终端设备查询功能
 * - add_device函数: 将终端设备添加到中继设备
 * - remove_device函数: 从中继设备移除终端设备
 *
 * 关键流程
 * ========
 * 1. 中继设备查询流程:
 *    - 根据过滤条件构建查询
 *    - 应用分页参数(limit和offset)
 *    - 执行查询并返回中继设备列表
 *
 * 2. 终端设备关联流程:
 *    - 验证中继设备和终端设备的存在性
 *    - 检查中继设备关联的终端设备数量是否超过限制
 *    - 创建中继设备和终端设备之间的关联
 *    - 更新中继设备的过滤列表
 *
 * 3. 终端设备移除流程:
 *    - 验证中继设备和终端设备的关联存在性
 *    - 删除中继设备和终端设备之间的关联
 *    - 更新中继设备的过滤列表
 *
 * 注意事项
 * ========
 * - 设备数量限制: 每个中继设备最多可以关联15个终端设备
 * - 过滤列表: 中继设备使用过滤列表来决定哪些上行消息需要转发
 * - 上行限制: 终端设备通过中继的上行消息受到速率限制
 * - 设备类型: 中继设备必须在设备配置文件中标记为中继
 * - 性能考虑: 中继设备查询和更新操作的性能对系统响应时间有影响
 * - 测试支持: 包含测试模块，用于验证中继设备管理功能
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use tracing::info;
use uuid::Uuid;

use lrwn::{DevAddr, EUI64};

use super::schema::{device, device_profile, relay_device};
use super::{db_transaction, device::Device, error::Error, fields, get_async_db_conn};

// This is set to 15, because the FilterList must contain a "catch-all" record to filter all
// uplinks that do not match the remaining records. This means that we can use 16 - 1 FilterList
// entries effectively.
const RELAY_MAX_DEVICES: i64 = 15;

#[derive(Default, Clone)]
pub struct RelayFilters {
    pub application_id: Option<Uuid>,
}

#[derive(Queryable, PartialEq, Eq, Debug)]
pub struct RelayListItem {
    pub dev_eui: EUI64,
    pub name: String,
}

#[derive(Default, Clone)]
pub struct DeviceFilters {
    pub relay_dev_eui: Option<EUI64>,
}

#[derive(Queryable, PartialEq, Eq, Debug)]
pub struct DeviceListItem {
    pub dev_eui: EUI64,
    pub join_eui: EUI64,
    pub dev_addr: Option<DevAddr>,
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub relay_ed_uplink_limit_bucket_size: i16,
    pub relay_ed_uplink_limit_reload_rate: i16,
}

pub async fn get_relay_count(filters: &RelayFilters) -> Result<i64, Error> {
    let mut q = device::dsl::device
        .select(dsl::count_star())
        .inner_join(device_profile::table)
        .filter(device_profile::dsl::is_relay.eq(true))
        .into_boxed();

    if let Some(application_id) = &filters.application_id {
        q = q.filter(device::dsl::application_id.eq(fields::Uuid::from(application_id)));
    }

    Ok(q.first(&mut get_async_db_conn().await?).await?)
}

pub async fn list_relays(
    limit: i64,
    offset: i64,
    filters: &RelayFilters,
) -> Result<Vec<RelayListItem>, Error> {
    let mut q = device::dsl::device
        .inner_join(device_profile::table)
        .select((device::dev_eui, device::name))
        .filter(device_profile::dsl::is_relay.eq(true))
        .into_boxed();

    if let Some(application_id) = &filters.application_id {
        q = q.filter(device::dsl::application_id.eq(fields::Uuid::from(application_id)));
    }

    q.order_by(device::dsl::name)
        .limit(limit)
        .offset(offset)
        .load(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, "".into()))
}

pub async fn get_device_count(filters: &DeviceFilters) -> Result<i64, Error> {
    let mut q = relay_device::dsl::relay_device
        .select(dsl::count_star())
        .into_boxed();

    if let Some(relay_dev_eui) = &filters.relay_dev_eui {
        q = q.filter(relay_device::dsl::relay_dev_eui.eq(relay_dev_eui));
    }

    q.first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, "".into()))
}

pub async fn list_devices(
    limit: i64,
    offset: i64,
    filters: &DeviceFilters,
) -> Result<Vec<DeviceListItem>, Error> {
    let mut q = relay_device::dsl::relay_device
        .inner_join(device::table.on(relay_device::dsl::dev_eui.eq(device::dsl::dev_eui)))
        .inner_join(
            device_profile::table.on(device::dsl::device_profile_id.eq(device_profile::dsl::id)),
        )
        .select((
            relay_device::dev_eui,
            device::join_eui,
            device::dev_addr,
            relay_device::created_at,
            device::name,
            device_profile::relay_ed_uplink_limit_bucket_size,
            device_profile::relay_ed_uplink_limit_reload_rate,
        ))
        .into_boxed();

    if let Some(relay_dev_eui) = &filters.relay_dev_eui {
        q = q.filter(relay_device::dsl::relay_dev_eui.eq(relay_dev_eui));
    }

    q.order_by(device::dsl::name)
        .limit(limit)
        .offset(offset)
        .load(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, "".into()))
}

pub async fn add_device(relay_dev_eui: EUI64, device_dev_eui: EUI64) -> Result<(), Error> {
    let mut c = get_async_db_conn().await?;
    db_transaction::<(), Error, _>(&mut c, |c| {
        Box::pin(async move {
            let query = device::dsl::device.find(&relay_dev_eui);
            // We lock the relay device to avoid race-conditions in the validation.
            #[cfg(feature = "postgres")]
            let query = query.for_update();
            let rd: Device = query
                .get_result(c)
                .await
                .map_err(|e| Error::from_diesel(e, relay_dev_eui.to_string()))?;

            // Is the given relay_dev_eui a Relay?
            let rdp: super::device_profile::DeviceProfile = device_profile::dsl::device_profile
                .find(&rd.device_profile_id)
                .get_result(c)
                .await
                .map_err(|e| Error::from_diesel(e, rd.device_profile_id.to_string()))?;
            if !rdp.is_relay {
                return Err(Error::Validation("Device is not a relay".to_string()));
            }

            // Validate that relay and device are under the same application.
            let d: Device = device::dsl::device
                .find(&device_dev_eui)
                .get_result(c)
                .await
                .map_err(|e| Error::from_diesel(e, device_dev_eui.to_string()))?;

            if rd.application_id != d.application_id {
                return Err(Error::Validation(
                    "Relay and device must be under the same application".into(),
                ));
            }

            // Validate that the relay and device are under the same region.
            let dp: super::device_profile::DeviceProfile = device_profile::dsl::device_profile
                .find(&d.device_profile_id)
                .get_result(c)
                .await
                .map_err(|e| Error::from_diesel(e, d.device_profile_id.to_string()))?;
            if rdp.region != dp.region {
                return Err(Error::Validation(
                    "Relay and device must be under the same region".into(),
                ));
            }

            // Validate that the device is not a relay.
            if dp.is_relay {
                return Err(Error::Validation("Can not add relay to a relay".into()));
            }

            // Validate max. number of devices.
            let count: i64 = relay_device::dsl::relay_device
                .select(dsl::count_star())
                .filter(relay_device::dsl::relay_dev_eui.eq(&relay_dev_eui))
                .first(c)
                .await
                .map_err(|e| Error::from_diesel(e, "".into()))?;

            if count > RELAY_MAX_DEVICES {
                return Err(Error::Validation(format!(
                    "Max number of devices that can be added to a relay is {}",
                    RELAY_MAX_DEVICES
                )));
            }

            let _ = diesel::insert_into(relay_device::table)
                .values((
                    relay_device::relay_dev_eui.eq(&relay_dev_eui),
                    relay_device::dev_eui.eq(&device_dev_eui),
                    relay_device::created_at.eq(Utc::now()),
                ))
                .execute(c)
                .await
                .map_err(|e| Error::from_diesel(e, "".into()))?;

            Ok(())
        })
    })
    .await?;

    info!(relay_dev_eui = %relay_dev_eui, device_dev_eui = %device_dev_eui, "Device added to relay");

    Ok(())
}

pub async fn remove_device(relay_dev_eui: EUI64, device_dev_eui: EUI64) -> Result<(), Error> {
    let ra = diesel::delete(
        relay_device::dsl::relay_device
            .filter(relay_device::relay_dev_eui.eq(&relay_dev_eui))
            .filter(relay_device::dev_eui.eq(&device_dev_eui)),
    )
    .execute(&mut get_async_db_conn().await?)
    .await?;
    if ra == 0 {
        return Err(Error::NotFound(format!(
            "relay_dev_eui: {}, device_dev_eui: {}",
            relay_dev_eui, device_dev_eui
        )));
    }

    info!(relay_dev_eui = %relay_dev_eui, device_dev_eui = %device_dev_eui, "Device removed from relay");

    Ok(())
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::storage;
    use crate::test;

    #[tokio::test]
    async fn test_relay() {
        let _guard = test::prepare().await;

        let dp = storage::device_profile::test::create_device_profile(None).await;
        let dp_relay = storage::device_profile::create(storage::device_profile::DeviceProfile {
            tenant_id: dp.tenant_id,
            name: "relay".into(),
            is_relay: true,
            ..Default::default()
        })
        .await
        .unwrap();

        let d = storage::device::test::create_device(
            EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            None,
        )
        .await;

        let d_relay = storage::device::test::create_device(
            EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
            dp_relay.id.into(),
            Some(d.application_id.into()),
        )
        .await;

        let d_other_app = storage::device::test::create_device(
            EUI64::from_be_bytes([3, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            None,
        )
        .await;

        let d_other_same_app = storage::device::test::create_device(
            EUI64::from_be_bytes([4, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            Some(d.application_id.into()),
        )
        .await;

        // relay count
        let relay_count = get_relay_count(&RelayFilters {
            application_id: Some(d_relay.application_id.into()),
        })
        .await
        .unwrap();
        assert_eq!(1, relay_count);

        // relay list
        let relay_list = list_relays(
            10,
            0,
            &RelayFilters {
                application_id: Some(d_relay.application_id.into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(1, relay_list.len());
        assert_eq!(d_relay.dev_eui, relay_list[0].dev_eui);

        // get device count (no devices)
        let device_count = get_device_count(&DeviceFilters {
            relay_dev_eui: Some(d_relay.dev_eui),
        })
        .await
        .unwrap();
        assert_eq!(0, device_count);

        // device list (no devices)
        let device_list = list_devices(
            10,
            0,
            &DeviceFilters {
                relay_dev_eui: Some(d_relay.dev_eui),
            },
        )
        .await
        .unwrap();
        assert_eq!(0, device_list.len());

        // add device from other app errors
        assert!(add_device(d_relay.dev_eui, d_other_app.dev_eui)
            .await
            .is_err());

        // add relay to relay errors
        assert!(add_device(d_relay.dev_eui, d_relay.dev_eui).await.is_err());

        // add to device that isn't relay errors
        assert!(add_device(d_other_same_app.dev_eui, d.dev_eui)
            .await
            .is_err());

        // add device to relay
        add_device(d_relay.dev_eui, d.dev_eui).await.unwrap();

        // get device count
        let device_count = get_device_count(&DeviceFilters {
            relay_dev_eui: Some(d_relay.dev_eui),
        })
        .await
        .unwrap();
        assert_eq!(1, device_count);

        // device list
        let device_list = list_devices(
            10,
            0,
            &DeviceFilters {
                relay_dev_eui: Some(d_relay.dev_eui),
            },
        )
        .await
        .unwrap();
        assert_eq!(1, device_list.len());
        assert_eq!(d.dev_eui, device_list[0].dev_eui);

        // remove device
        remove_device(d_relay.dev_eui, d.dev_eui).await.unwrap();
        assert!(remove_device(d_relay.dev_eui, d.dev_eui).await.is_err());
    }
}

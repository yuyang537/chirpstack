/*
 * 模块概述
 * ========
 * device_queue模块提供了用于管理LoRaWAN设备下行消息队列的存储和检索功能。设备队列
 * 存储了待发送给设备的下行消息，包括确认型和非确认型消息、加密状态、过期时间等信息。
 * 该模块确保了下行消息的可靠传递和顺序处理，是ChirpStack下行通信的核心组件。
 * 
 * 该模块使用关系型数据库(PostgreSQL或SQLite)作为存储后端，提供了完整的队列操作功能，
 * 包括入队、出队、查询和清空等。设备队列项通过UUID进行唯一标识，并与设备EUI关联，
 * 确保每个设备的下行消息能够被正确管理。
 *
 * 文件功能
 * ========
 * 本文件(device_queue.rs)实现了设备下行消息队列的存储和管理功能，提供了以下主要功能：
 * 1. 将下行消息入队
 * 2. 获取和更新队列项
 * 3. 获取设备的下一个待发送消息
 * 4. 管理消息的待处理状态和超时
 * 5. 清空设备的消息队列
 *
 * 主要组件
 * ========
 * - DeviceQueueItem结构体: 定义设备队列项数据模型，包含消息内容和状态
 * - enqueue_item函数: 将新的下行消息添加到队列
 * - get_item/update_item/delete_item函数: 提供基本的队列项操作
 * - get_next_for_dev_eui函数: 获取设备的下一个待发送消息
 * - get_for_dev_eui函数: 获取设备的所有队列消息
 * - flush_for_dev_eui函数: 清空设备的消息队列
 * - get_pending_for_dev_eui函数: 获取设备的待处理消息
 * - get_max_f_cnt_down函数: 获取设备队列中的最大下行帧计数器
 *
 * 关键流程
 * ========
 * 1. 下行消息入队流程:
 *    - 验证队列项参数的有效性
 *    - 将队列项插入数据库
 *    - 返回创建的队列项，包括生成的UUID
 *
 * 2. 下行消息出队流程:
 *    - 查询设备的下一个未处理消息
 *    - 更新消息状态为待处理
 *    - 设置消息超时时间
 *    - 返回消息内容用于下行传输
 *
 * 3. 消息确认/超时处理流程:
 *    - 根据设备响应或超时情况
 *    - 删除已确认或超时的消息
 *    - 处理未确认消息的重传
 *
 * 注意事项
 * ========
 * - 消息顺序: 队列确保消息按照先进先出(FIFO)顺序处理
 * - 帧计数器: 加密消息需要设置正确的下行帧计数器
 * - 消息超时: 待处理消息设置超时时间，防止无限等待
 * - 消息过期: 支持设置消息的过期时间，过期消息不会被发送
 * - 队列限制: 每个设备的队列大小可能受到限制
 * - 性能考虑: 队列操作频繁，查询性能至关重要
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use tracing::info;
use uuid::Uuid;

use super::schema::device_queue_item;
use super::{error::Error, fields, get_async_db_conn};
use lrwn::EUI64;

#[derive(Queryable, Insertable, PartialEq, Eq, Debug, Clone)]
#[diesel(table_name = device_queue_item)]
pub struct DeviceQueueItem {
    pub id: fields::Uuid,
    pub dev_eui: EUI64,
    pub created_at: DateTime<Utc>,
    pub f_port: i16,
    pub confirmed: bool,
    pub data: Vec<u8>,
    pub is_pending: bool,
    pub f_cnt_down: Option<i64>,
    pub timeout_after: Option<DateTime<Utc>>,
    pub is_encrypted: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

impl DeviceQueueItem {
    fn validate(&self) -> Result<(), Error> {
        if self.f_port == 0 || self.f_port > 255 {
            return Err(Error::Validation(
                "FPort must be between 1 - 255".to_string(),
            ));
        }

        if self.is_encrypted && self.f_cnt_down.is_none() {
            return Err(Error::Validation(
                "FCntDown must be set for encrypted queue-items".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for DeviceQueueItem {
    fn default() -> Self {
        let now = Utc::now();

        DeviceQueueItem {
            id: Uuid::new_v4().into(),
            dev_eui: EUI64::from_be_bytes([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            created_at: now,
            f_port: 0,
            confirmed: false,
            data: Vec::new(),
            is_pending: false,
            f_cnt_down: None,
            timeout_after: None,
            is_encrypted: false,
            expires_at: None,
        }
    }
}

pub async fn enqueue_item(qi: DeviceQueueItem) -> Result<DeviceQueueItem, Error> {
    qi.validate()?;

    let qi: DeviceQueueItem = diesel::insert_into(device_queue_item::table)
        .values(&qi)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, qi.id.to_string()))?;
    info!(id = %qi.id, dev_eui = %qi.dev_eui, "Device queue-item enqueued");
    Ok(qi)
}

pub async fn get_item(id: &Uuid) -> Result<DeviceQueueItem, Error> {
    let qi = device_queue_item::dsl::device_queue_item
        .find(&fields::Uuid::from(id))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;
    Ok(qi)
}

pub async fn update_item(qi: DeviceQueueItem) -> Result<DeviceQueueItem, Error> {
    let qi: DeviceQueueItem =
        diesel::update(device_queue_item::dsl::device_queue_item.find(&qi.id))
            .set((
                device_queue_item::is_pending.eq(&qi.is_pending),
                device_queue_item::f_cnt_down.eq(&qi.f_cnt_down),
                device_queue_item::timeout_after.eq(&qi.timeout_after),
            ))
            .get_result(&mut get_async_db_conn().await?)
            .await
            .map_err(|e| Error::from_diesel(e, qi.id.to_string()))?;
    info!(id = %qi.id, dev_eui = %qi.dev_eui, "Device queue-item updated");
    Ok(qi)
}

pub async fn delete_item(id: &Uuid) -> Result<(), Error> {
    let ra =
        diesel::delete(device_queue_item::dsl::device_queue_item.find(&fields::Uuid::from(id)))
            .execute(&mut get_async_db_conn().await?)
            .await?;
    if ra == 0 {
        return Err(Error::NotFound(id.to_string()));
    }
    info!(id = %id, "Device queue-item deleted");
    Ok(())
}

/// It returns the device queue-item and a bool indicating if there are more items in the queue.
pub async fn get_next_for_dev_eui(dev_eui: &EUI64) -> Result<(DeviceQueueItem, bool), Error> {
    let items: Vec<DeviceQueueItem> = device_queue_item::dsl::device_queue_item
        .filter(device_queue_item::dev_eui.eq(&dev_eui))
        .order_by(device_queue_item::created_at)
        .limit(2)
        .load(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;

    // Return NotFound on empty Vec.
    if items.is_empty() {
        return Err(Error::NotFound(dev_eui.to_string()));
    }

    // In case the transmission is pending and hasn't timed-out yet, do not
    // return it.
    if items[0].is_pending {
        if let Some(timeout_after) = &items[0].timeout_after {
            if timeout_after > &Utc::now() {
                return Err(Error::NotFound(dev_eui.to_string()));
            }
        }
    }

    // Return first item and bool indicating if there are more items in the queue.
    Ok((items[0].clone(), items.len() > 1))
}

pub async fn get_for_dev_eui(dev_eui: &EUI64) -> Result<Vec<DeviceQueueItem>, Error> {
    let items = device_queue_item::dsl::device_queue_item
        .filter(device_queue_item::dev_eui.eq(&dev_eui))
        .order_by(device_queue_item::created_at)
        .load(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    Ok(items)
}

pub async fn flush_for_dev_eui(dev_eui: &EUI64) -> Result<(), Error> {
    let count: usize = diesel::delete(
        device_queue_item::dsl::device_queue_item.filter(device_queue_item::dev_eui.eq(&dev_eui)),
    )
    .execute(&mut get_async_db_conn().await?)
    .await
    .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    info!(dev_eui = %dev_eui, count = count, "Device queue flushed");
    Ok(())
}

pub async fn get_pending_for_dev_eui(dev_eui: &EUI64) -> Result<DeviceQueueItem, Error> {
    let qi = device_queue_item::dsl::device_queue_item
        .filter(
            device_queue_item::dev_eui
                .eq(&dev_eui)
                .and(device_queue_item::is_pending.eq(true)),
        )
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    Ok(qi)
}

pub async fn get_max_f_cnt_down(dev_eui: EUI64) -> Result<Option<i64>, Error> {
    Ok(device_queue_item::dsl::device_queue_item
        .select(dsl::max(device_queue_item::f_cnt_down))
        .filter(device_queue_item::dsl::dev_eui.eq(dev_eui))
        .first(&mut get_async_db_conn().await?)
        .await?)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::storage;
    use crate::test;

    #[tokio::test]
    async fn test_queue_item() {
        let _guard = test::prepare().await;
        let dp = storage::device_profile::test::create_device_profile(None).await;
        let d = storage::device::test::create_device(
            EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            None,
        )
        .await;

        // invalid fport
        let qi = DeviceQueueItem {
            dev_eui: d.dev_eui,
            f_port: 0,
            data: vec![0x01, 0x02, 0x03],
            ..Default::default()
        };
        assert!(enqueue_item(qi).await.is_err());

        let qi = DeviceQueueItem {
            dev_eui: d.dev_eui,
            f_port: 256,
            data: vec![0x01, 0x02, 0x03],
            ..Default::default()
        };
        assert!(enqueue_item(qi).await.is_err());

        // create
        let mut qi = DeviceQueueItem {
            dev_eui: d.dev_eui,
            f_port: 10,
            data: vec![0x01, 0x02, 0x03],
            ..Default::default()
        };
        qi = enqueue_item(qi).await.unwrap();

        // get
        let qi_get = get_item(&qi.id).await.unwrap();
        assert_eq!(qi, qi_get);

        // get for dev eui
        let queue = get_for_dev_eui(&d.dev_eui).await.unwrap();
        assert_eq!(qi, queue[0]);

        // next next queue item for dev eui
        let resp = get_next_for_dev_eui(&d.dev_eui).await.unwrap();
        assert_eq!(qi, resp.0);
        assert!(!resp.1);

        // update
        qi.is_pending = true;
        qi = update_item(qi).await.unwrap();
        let qi_get = get_item(&qi.id).await.unwrap();
        assert_eq!(qi, qi_get);

        // delete
        delete_item(&qi.id).await.unwrap();
        assert!(delete_item(&qi.id).await.is_err());
    }

    #[tokio::test]
    async fn test_flush_queue() {
        let _guard = test::prepare().await;
        let dp = storage::device_profile::test::create_device_profile(None).await;
        let d = storage::device::test::create_device(
            EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            None,
        )
        .await;

        // create
        let mut qi = DeviceQueueItem {
            dev_eui: d.dev_eui,
            f_port: 10,
            data: vec![0x01, 0x02, 0x03],
            ..Default::default()
        };
        qi = enqueue_item(qi).await.unwrap();

        // flush
        flush_for_dev_eui(&d.dev_eui).await.unwrap();
        assert!(delete_item(&qi.id).await.is_err());
    }

    #[tokio::test]
    async fn test_get_max_f_cnt_down() {
        let _guard = test::prepare().await;
        let dp = storage::device_profile::test::create_device_profile(None).await;
        let d = storage::device::test::create_device(
            EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            dp.id.into(),
            None,
        )
        .await;

        // create
        let mut qi = DeviceQueueItem {
            dev_eui: d.dev_eui,
            f_port: 10,
            data: vec![0x01, 0x02, 0x03],
            ..Default::default()
        };
        qi = enqueue_item(qi).await.unwrap();

        // No max_f_cnt.
        let max_f_cnt = get_max_f_cnt_down(d.dev_eui).await.unwrap();
        assert_eq!(None, max_f_cnt);

        qi.f_cnt_down = Some(10);
        update_item(qi).await.unwrap();
        let max_f_cnt = get_max_f_cnt_down(d.dev_eui).await.unwrap();
        assert_eq!(Some(10), max_f_cnt);
    }
}

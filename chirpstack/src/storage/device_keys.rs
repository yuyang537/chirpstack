/*
 * 模块概述
 * ========
 * device_keys模块提供了用于管理LoRaWAN设备密钥的存储和检索功能。设备密钥是LoRaWAN安全
 * 架构的核心组件，包括网络密钥(NwkKey)和应用密钥(AppKey)，用于设备激活、消息加密和
 * 完整性保护。此外，该模块还管理设备随机数(DevNonces)和连接计数器(JoinNonce)，用于
 * 防止重放攻击和确保安全的设备激活过程。
 * 
 * 该模块使用关系型数据库(PostgreSQL或SQLite)作为存储后端，提供了完整的CRUD操作和
 * 事务支持。设备密钥数据通过设备的EUI(扩展唯一标识符)进行索引，确保每个设备的密钥
 * 信息能够被安全地存储和检索。
 *
 * 文件功能
 * ========
 * 本文件(device_keys.rs)实现了设备密钥的存储和管理功能，提供了以下主要功能：
 * 1. 创建、读取、更新和删除设备密钥记录
 * 2. 管理设备随机数(DevNonces)，防止重放攻击
 * 3. 验证和递增连接计数器(JoinNonce)
 * 4. 支持OTAA(空中激活)过程中的安全验证
 *
 * 主要组件
 * ========
 * - DeviceKeys结构体: 定义设备密钥数据模型，包括EUI、密钥和计数器
 * - create/get/update/delete函数: 提供基本的CRUD操作
 * - set_dev_nonces函数: 更新设备的随机数集合
 * - validate_incr_join_and_store_dev_nonce函数: 验证设备随机数并递增连接计数器
 * - 测试模块: 提供单元测试和辅助函数
 *
 * 关键流程
 * ========
 * 1. 设备激活验证流程:
 *    - 设备发送Join请求，包含DevEUI、JoinEUI和DevNonce
 *    - 通过validate_incr_join_and_store_dev_nonce函数验证DevNonce是否已使用
 *    - 如果验证通过，递增JoinNonce并存储新的DevNonce
 *    - 返回更新后的DeviceKeys，用于生成会话密钥
 *
 * 2. 设备密钥管理流程:
 *    - 通过create函数创建新的设备密钥记录
 *    - 使用get函数检索现有设备密钥
 *    - 通过update函数更新设备密钥信息
 *    - 使用delete函数删除不再需要的设备密钥
 *
 * 注意事项
 * ========
 * - 安全性: 设备密钥是高度敏感的安全信息，需要妥善保护
 * - 数据一致性: 使用数据库事务确保DevNonce和JoinNonce的一致性
 * - 性能考虑: 设备激活过程中的密钥操作是性能关键路径
 * - 错误处理: 适当处理数据库连接和查询过程中的错误
 * - 测试支持: 包含测试模块，用于验证设备密钥管理功能
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tracing::info;

use lrwn::{AES128Key, EUI64};

use super::error::Error;
use super::schema::device_keys;
use super::{db_transaction, fields, get_async_db_conn};

#[derive(Queryable, Insertable, AsChangeset, PartialEq, Eq, Debug, Clone)]
#[diesel(table_name = device_keys)]
pub struct DeviceKeys {
    pub dev_eui: EUI64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub nwk_key: AES128Key,
    pub app_key: AES128Key,
    pub dev_nonces: fields::DevNonces,
    pub join_nonce: i32,
}

impl Default for DeviceKeys {
    fn default() -> Self {
        let now = Utc::now();

        DeviceKeys {
            dev_eui: EUI64::from_be_bytes([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            created_at: now,
            updated_at: now,
            nwk_key: AES128Key::from_bytes([
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ]),
            app_key: AES128Key::from_bytes([
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ]),
            dev_nonces: fields::DevNonces::default(),
            join_nonce: 0,
        }
    }
}

pub async fn create(dk: DeviceKeys) -> Result<DeviceKeys, Error> {
    let dk: DeviceKeys = diesel::insert_into(device_keys::table)
        .values(&dk)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dk.dev_eui.to_string()))?;
    info!(
        dev_eui = %dk.dev_eui,
        "Device-keys created"
    );
    Ok(dk)
}

pub async fn get(dev_eui: &EUI64) -> Result<DeviceKeys, Error> {
    let dk = device_keys::dsl::device_keys
        .find(&dev_eui)
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    Ok(dk)
}

pub async fn update(dk: DeviceKeys) -> Result<DeviceKeys, Error> {
    let dk: DeviceKeys = diesel::update(device_keys::dsl::device_keys.find(&dk.dev_eui))
        .set(&dk)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dk.dev_eui.to_string()))?;
    info!(
        dev_eui = %dk.dev_eui,
        "Device-keys updated"
    );
    Ok(dk)
}

pub async fn delete(dev_eui: &EUI64) -> Result<(), Error> {
    let ra = diesel::delete(device_keys::dsl::device_keys.find(&dev_eui))
        .execute(&mut get_async_db_conn().await?)
        .await?;
    if ra == 0 {
        return Err(Error::NotFound(dev_eui.to_string()));
    }
    info!(
        dev_eui = %dev_eui,
        "Device-keys deleted"
    );
    Ok(())
}

pub async fn set_dev_nonces(
    dev_eui: EUI64,
    nonces: &fields::DevNonces,
) -> Result<DeviceKeys, Error> {
    let dk: DeviceKeys = diesel::update(device_keys::dsl::device_keys.find(dev_eui))
        .set(device_keys::dev_nonces.eq(nonces))
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;
    info!(
        dev_eui = %dev_eui,
        "Dev-nonces updated"
    );
    Ok(dk)
}

pub async fn validate_incr_join_and_store_dev_nonce(
    join_eui: EUI64,
    dev_eui: EUI64,
    dev_nonce: u16,
) -> Result<DeviceKeys, Error> {
    let mut c = get_async_db_conn().await?;
    let dk: DeviceKeys = db_transaction::<DeviceKeys, Error, _>(&mut c, |c| {
        Box::pin(async move {
            let query = device_keys::dsl::device_keys.find(&dev_eui);
            #[cfg(feature = "postgres")]
            let query = query.for_update();
            let mut dk: DeviceKeys = query
                .first(c)
                .await
                .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;

            if dk.dev_nonces.contains(join_eui, dev_nonce) {
                return Err(Error::InvalidDevNonce);
            }

            dk.dev_nonces.insert(join_eui, dev_nonce);
            dk.join_nonce += 1;

            diesel::update(device_keys::dsl::device_keys.find(&dev_eui))
                .set((
                    device_keys::updated_at.eq(Utc::now()),
                    device_keys::dev_nonces.eq(&dk.dev_nonces),
                    device_keys::join_nonce.eq(&dk.join_nonce),
                ))
                .get_result(c)
                .await
                .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))
        })
    })
    .await?;

    info!(dev_eui = %dev_eui, dev_nonce = dev_nonce, "Device-nonce validated, join-nonce incremented and stored");
    Ok(dk)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::storage;
    use crate::test;

    pub async fn reset_nonces(dev_eui: &EUI64) -> Result<DeviceKeys, Error> {
        let dk: DeviceKeys = diesel::update(device_keys::dsl::device_keys.find(&dev_eui))
            .set((
                device_keys::dev_nonces.eq(fields::DevNonces::default()),
                device_keys::join_nonce.eq(0),
            ))
            .get_result(&mut get_async_db_conn().await?)
            .await
            .map_err(|e| Error::from_diesel(e, dev_eui.to_string()))?;

        info!(
            dev_eui = %dev_eui,
            "Nonces reset"
        );
        Ok(dk)
    }

    pub async fn create_device_keys(dev_eui: Option<EUI64>) -> DeviceKeys {
        let dev_eui = match dev_eui {
            Some(v) => v,
            None => {
                let dp = storage::device_profile::test::create_device_profile(None).await;
                let a = storage::application::test::create_application(None).await;
                let d = storage::device::Device {
                    name: "test-dev".into(),
                    dev_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
                    application_id: a.id,
                    device_profile_id: dp.id,
                    ..Default::default()
                };

                let d = storage::device::create(d).await.unwrap();
                d.dev_eui
            }
        };

        let dk = DeviceKeys {
            dev_eui,
            ..Default::default()
        };

        create(dk).await.unwrap()
    }

    #[tokio::test]
    async fn test_device_keys() {
        let _guard = test::prepare().await;
        let mut dk = create_device_keys(None).await;

        // get
        let dk_get = get(&dk.dev_eui).await.unwrap();
        assert_eq!(dk, dk_get);

        // update
        dk.join_nonce = 10;
        dk = update(dk).await.unwrap();
        let dk_get = get(&dk.dev_eui).await.unwrap();
        assert_eq!(dk, dk_get);

        // delete
        delete(&dk.dev_eui).await.unwrap();
        assert!(delete(&dk.dev_eui).await.is_err());
    }
}

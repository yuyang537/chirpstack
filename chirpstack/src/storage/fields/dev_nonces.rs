/*
 * 模块概述
 * ========
 * DevNonces模块提供了一个用于管理LoRaWAN设备激活过程中使用的设备随机数(DevNonce)的
 * 数据结构。该模块解决了OTAA(空中激活)过程中防止重放攻击的关键安全需求，通过跟踪
 * 每个JoinEUI(连接服务器标识符)对应的已使用DevNonce列表，确保每个DevNonce只被使用一次。
 * 
 * 该模块实现了一个包装HashMap<EUI64, Vec<u16>>的DevNonces结构体，其中EUI64表示JoinEUI，
 * Vec<u16>存储该JoinEUI下已使用的DevNonce列表。同时，为该结构体实现了Diesel ORM所需的
 * 序列化和反序列化特性，使其可以在PostgreSQL的JSONB类型和SQLite的TEXT类型之间进行映射。
 *
 * 文件功能
 * ========
 * 本文件(dev_nonces.rs)实现了DevNonces类型及其相关特性，提供了以下主要功能：
 * 1. 定义DevNonces结构体，包装HashMap<EUI64, Vec<u16>>
 * 2. 提供检查和添加DevNonce的方法
 * 3. 实现Diesel ORM所需的序列化和反序列化特性
 * 4. 支持在不同数据库后端中存储和检索DevNonce数据
 *
 * 主要组件
 * ========
 * - DevNonces结构体: 包装HashMap<EUI64, Vec<u16>>类型
 * - contains方法: 检查特定JoinEUI下是否已存在某个DevNonce
 * - insert方法: 向特定JoinEUI的DevNonce列表中添加新的DevNonce
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 *
 * 关键流程
 * ========
 * 1. DevNonce验证流程:
 *    - 设备发送Join请求，包含JoinEUI和DevNonce
 *    - 通过contains方法检查该JoinEUI下是否已使用过该DevNonce
 *    - 如果已使用，拒绝Join请求（防止重放攻击）
 *    - 如果未使用，接受Join请求并通过insert方法记录该DevNonce
 *
 * 2. 数据库交互流程:
 *    - 从数据库读取JSONB(PostgreSQL)或TEXT(SQLite)类型的值
 *    - 通过FromSql特性将JSON数据解析为DevNonces类型
 *    - 应用程序使用DevNonces进行DevNonce验证
 *    - 通过ToSql特性将DevNonces序列化为JSON格式
 *    - 写入数据库的JSONB(PostgreSQL)或TEXT(SQLite)列
 *
 * 注意事项
 * ========
 * - 内存占用: 随着设备激活次数增加，DevNonce列表会不断增长
 * - 性能考虑: 对于活跃设备，DevNonce列表可能变得很大，影响查找性能
 * - 安全性: 正确实现DevNonce跟踪对防止重放攻击至关重要
 * - 数据库存储: JSON格式存储可能不如专用表效率高，但提供了更大的灵活性
 * - LoRaWAN规范: 实现需符合LoRaWAN规范对DevNonce处理的要求
 */

use std::collections::HashMap;

use diesel::backend::Backend;

use diesel::{deserialize, serialize};
#[cfg(feature = "postgres")]
use diesel::{pg::Pg, sql_types::Jsonb};
#[cfg(feature = "sqlite")]
use diesel::{sql_types::Text, sqlite::Sqlite};

use lrwn::EUI64;

#[derive(Default, Debug, Clone, PartialEq, Eq, AsExpression, FromSqlRow)]
#[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
pub struct DevNonces(HashMap<EUI64, Vec<u16>>);

impl DevNonces {
    pub fn contains(&self, join_eui: EUI64, dev_nonce: u16) -> bool {
        if let Some(v) = self.0.get(&join_eui) {
            v.contains(&dev_nonce)
        } else {
            false
        }
    }

    pub fn insert(&mut self, join_eui: EUI64, dev_nonce: u16) {
        self.0.entry(join_eui).or_default().push(dev_nonce)
    }
}

#[cfg(feature = "postgres")]
impl deserialize::FromSql<Jsonb, Pg> for DevNonces {
    fn from_sql(value: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as deserialize::FromSql<Jsonb, Pg>>::from_sql(value)?;
        let dev_nonces: HashMap<EUI64, Vec<u16>> = serde_json::from_value(value)?;
        Ok(DevNonces(dev_nonces))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Jsonb, Pg> for DevNonces {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(&self.0)?;
        <serde_json::Value as serialize::ToSql<Jsonb, Pg>>::to_sql(&value, &mut out.reborrow())
    }
}

#[cfg(feature = "sqlite")]
impl deserialize::FromSql<Text, Sqlite> for DevNonces
where
    *const str: deserialize::FromSql<Text, Sqlite>,
{
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s =
            <*const str as deserialize::FromSql<diesel::sql_types::Text, Sqlite>>::from_sql(value)?;
        let dev_nonces: HashMap<EUI64, Vec<u16>> = serde_json::from_str(unsafe { &*s })?;
        Ok(DevNonces(dev_nonces))
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Text, Sqlite> for DevNonces {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(serialize::IsNull::No)
    }
}

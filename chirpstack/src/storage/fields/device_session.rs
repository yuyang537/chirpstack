/*
 * 模块概述
 * ========
 * DeviceSession模块提供了一个用于管理LoRaWAN设备会话状态的数据结构包装器。该模块解决了
 * 在ChirpStack系统中持久化和检索设备会话信息的需求，这些信息对于维护设备的网络连接、
 * 安全上下文和通信参数至关重要。
 * 
 * 该模块包装了由Protocol Buffers生成的internal::DeviceSession类型，并为其实现了
 * Diesel ORM所需的序列化和反序列化特性，使其可以在PostgreSQL和SQLite的Binary类型中
 * 存储和检索。通过这种方式，设备会话数据可以高效地序列化为二进制格式存储在数据库中，
 * 同时在应用程序中以结构化对象的形式进行操作。
 *
 * 文件功能
 * ========
 * 本文件(device_session.rs)实现了DeviceSession类型及其相关特性，提供了以下主要功能：
 * 1. 定义DeviceSession结构体，包装Protocol Buffers生成的internal::DeviceSession
 * 2. 提供创建和转换方法，便于与原始internal::DeviceSession交互
 * 3. 实现Diesel ORM所需的序列化和反序列化特性
 * 4. 通过Deref和DerefMut特性，允许透明地访问底层DeviceSession的方法和字段
 *
 * 主要组件
 * ========
 * - DeviceSession结构体: 包装internal::DeviceSession类型
 * - new方法: 从internal::DeviceSession创建DeviceSession实例
 * - From/Into实现: 支持与internal::DeviceSession的相互转换
 * - Deref/DerefMut实现: 允许透明地访问底层DeviceSession的方法和字段
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 *
 * 关键流程
 * ========
 * 1. 设备会话创建和更新流程:
 *    - 设备成功加入网络或激活后，创建新的DeviceSession实例
 *    - 设置设备会话参数，如密钥、计数器、MAC命令等
 *    - 通过ToSql特性将DeviceSession序列化为二进制格式
 *    - 存储到数据库的Binary列中
 *
 * 2. 设备会话检索和使用流程:
 *    - 从数据库读取Binary类型的值
 *    - 通过FromSql特性将二进制数据解析为DeviceSession类型
 *    - 应用程序通过Deref/DerefMut访问和修改设备会话参数
 *    - 处理上行消息、准备下行消息、管理MAC命令等
 *
 * 注意事项
 * ========
 * - 二进制序列化: 使用Protocol Buffers进行高效的二进制序列化
 * - 版本兼容性: 需要注意Protocol Buffers消息格式变更的向后兼容性
 * - 数据大小: 设备会话可能包含大量状态信息，影响存储空间和性能
 * - 安全性: 设备会话包含敏感的安全密钥和参数，需要妥善保护
 * - 数据库兼容性: 二进制存储在不同数据库后端中的处理方式可能有所不同
 */

use std::io::Cursor;
use std::ops::{Deref, DerefMut};

use diesel::backend::Backend;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
use diesel::sql_types::Binary;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::{deserialize, serialize};
use prost::Message;

use chirpstack_api::internal;

#[derive(Debug, Clone, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = diesel::sql_types::Binary)]
pub struct DeviceSession(internal::DeviceSession);

impl DeviceSession {
    pub fn new(m: internal::DeviceSession) -> Self {
        DeviceSession(m)
    }
}

impl std::convert::From<internal::DeviceSession> for DeviceSession {
    fn from(u: internal::DeviceSession) -> Self {
        Self(u)
    }
}

impl std::convert::From<&internal::DeviceSession> for DeviceSession {
    fn from(u: &internal::DeviceSession) -> Self {
        Self::from(u.clone())
    }
}

impl std::convert::From<DeviceSession> for internal::DeviceSession {
    fn from(val: DeviceSession) -> Self {
        val.0
    }
}

impl Deref for DeviceSession {
    type Target = internal::DeviceSession;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DeviceSession {
    fn deref_mut(&mut self) -> &mut internal::DeviceSession {
        &mut self.0
    }
}

impl<DB> deserialize::FromSql<Binary, DB> for DeviceSession
where
    DB: Backend,
    *const [u8]: deserialize::FromSql<Binary, DB>,
{
    fn from_sql(value: <DB as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let bindata = <*const [u8] as deserialize::FromSql<Binary, DB>>::from_sql(value)?;
        let ds = internal::DeviceSession::decode(&mut Cursor::new(unsafe { &*bindata }))?;
        Ok(DeviceSession(ds))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Binary, Pg> for DeviceSession {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Pg>) -> serialize::Result {
        let encoded = self.encode_to_vec();
        <Vec<u8> as serialize::ToSql<Binary, Pg>>::to_sql(&encoded, &mut out.reborrow())
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Binary, Sqlite> for DeviceSession {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.encode_to_vec());
        Ok(serialize::IsNull::No)
    }
}

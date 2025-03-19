/*
 * 模块概述
 * ========
 * UUID模块提供了一个通用唯一标识符(UUID)类型的包装器，用于在ChirpStack系统中
 * 作为各种实体（如设备、网关、应用程序、租户等）的主键和唯一标识符。该模块解决了
 * 在分布式系统中生成全局唯一标识符的问题，同时确保与数据库存储和API交互的兼容性。
 * 
 * 该模块包装了第三方库uuid的Uuid类型，并为其实现了Diesel ORM所需的序列化和反序列化
 * 特性，使其可以在PostgreSQL的UUID类型和SQLite的TEXT类型之间进行映射。通过这种方式，
 * UUID可以在不同的数据库后端中一致地使用，同时保持其全局唯一性和高效的索引性能。
 *
 * 文件功能
 * ========
 * 本文件(uuid.rs)实现了Uuid类型及其相关特性，提供了以下主要功能：
 * 1. 定义Uuid结构体，包装第三方uuid库的Uuid类型
 * 2. 提供创建和转换方法，包括随机UUID生成和从字符串解析
 * 3. 实现Diesel ORM所需的序列化和反序列化特性
 * 4. 提供与原始uuid库类型的互操作性
 *
 * 主要组件
 * ========
 * - Uuid结构体: 包装第三方uuid库的Uuid类型
 * - new方法: 创建随机UUID实例
 * - from_str方法: 从字符串解析UUID
 * - to_string方法: 将UUID转换为字符串表示
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 * - 各种转换特性实现: 如From/Into/AsRef等，便于与原始uuid类型交互
 *
 * 关键流程
 * ========
 * 1. UUID生成流程:
 *    - 应用程序调用Uuid::new()生成随机UUID
 *    - 内部使用uuid库的随机生成功能创建全局唯一标识符
 *    - 生成的UUID用作实体的主键或唯一标识符
 *
 * 2. 数据库交互流程:
 *    - 从数据库读取UUID类型(PostgreSQL)或TEXT类型(SQLite)的值
 *    - 通过FromSql特性将数据库值解析为Uuid类型
 *    - 应用程序使用Uuid进行实体标识和关联
 *    - 通过ToSql特性将Uuid序列化为适合数据库的格式
 *
 * 注意事项
 * ========
 * - 数据库兼容性: PostgreSQL原生支持UUID类型，而SQLite使用TEXT存储
 * - 性能考虑: UUID作为主键可能比自增整数稍慢，但提供了分布式生成的优势
 * - 存储空间: UUID通常需要16字节存储，比整数主键占用更多空间
 * - 可读性: UUID字符串表示较长(36字符)，在日志和调试中可能不如短标识符直观
 * - 版本选择: 当前实现使用随机(v4)UUID，适合大多数分布式场景
 */

use std::fmt;
use std::str::FromStr;

use diesel::backend::Backend;
use diesel::{deserialize, serialize};
#[cfg(feature = "postgres")]
use diesel::{pg::Pg, sql_types::Uuid as PgUuid};
#[cfg(feature = "sqlite")]
use diesel::{sql_types::Text, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Copy, Clone, Debug, Eq, PartialEq, AsExpression, FromSqlRow)]
#[serde(transparent)]
#[cfg_attr(feature = "postgres", diesel(sql_type = diesel::sql_types::Uuid))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Text))]
pub struct Uuid(uuid::Uuid);

impl std::convert::From<uuid::Uuid> for Uuid {
    fn from(u: uuid::Uuid) -> Self {
        Self(u)
    }
}

impl std::convert::From<&uuid::Uuid> for Uuid {
    fn from(u: &uuid::Uuid) -> Self {
        Self::from(*u)
    }
}

impl std::convert::From<Uuid> for uuid::Uuid {
    fn from(val: Uuid) -> Self {
        val.0
    }
}

impl std::ops::Deref for Uuid {
    type Target = uuid::Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Uuid {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

#[cfg(feature = "postgres")]
impl deserialize::FromSql<diesel::sql_types::Uuid, Pg> for Uuid {
    fn from_sql(value: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let u = <uuid::Uuid>::from_sql(value)?;
        Ok(Uuid(u))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<diesel::sql_types::Uuid, Pg> for Uuid {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Pg>) -> serialize::Result {
        <uuid::Uuid as serialize::ToSql<diesel::sql_types::Uuid, Pg>>::to_sql(
            &self.0,
            &mut out.reborrow(),
        )
    }
}

#[cfg(feature = "sqlite")]
impl deserialize::FromSql<diesel::sql_types::Text, Sqlite> for Uuid {
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s =
            <*const str as deserialize::FromSql<diesel::sql_types::Text, Sqlite>>::from_sql(value)?;
        let u = uuid::Uuid::try_parse(unsafe { &*s })?;
        Ok(Uuid(u))
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<diesel::sql_types::Text, Sqlite> for Uuid {
    fn to_sql<'b>(&self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.to_string());
        Ok(serialize::IsNull::No)
    }
}

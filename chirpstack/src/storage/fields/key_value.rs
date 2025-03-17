/*
 * 模块概述
 * ========
 * KeyValue模块提供了一个键值对映射类型的包装器，用于在ChirpStack系统中存储和处理
 * 元数据、标签和配置信息等非结构化数据。该模块解决了在关系型数据库中存储灵活的
 * 键值对数据的问题，使应用程序可以轻松地为各种实体（如设备、网关、应用等）添加
 * 自定义属性，而无需修改数据库模式。
 * 
 * 该模块包装了Rust标准库的HashMap<String, String>类型，并为其实现了Diesel ORM所需的
 * 序列化和反序列化特性，使其可以在PostgreSQL的JSONB类型和SQLite的TEXT类型之间进行映射。
 * 通过这种方式，键值对数据可以作为JSON对象存储在数据库中，同时在应用程序中以HashMap
 * 的形式进行操作。
 *
 * 文件功能
 * ========
 * 本文件(key_value.rs)实现了KeyValue类型及其相关特性，提供了以下主要功能：
 * 1. 定义KeyValue结构体，包装HashMap<String, String>
 * 2. 提供创建和转换方法，便于与HashMap交互
 * 3. 实现Diesel ORM所需的序列化和反序列化特性
 * 4. 通过Deref和DerefMut特性，允许直接使用HashMap的方法
 *
 * 主要组件
 * ========
 * - KeyValue结构体: 包装HashMap<String, String>类型
 * - new方法: 从HashMap创建KeyValue实例
 * - into_hashmap方法: 将KeyValue转换回HashMap
 * - Deref/DerefMut实现: 允许透明地访问底层HashMap的方法
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 *
 * 关键流程
 * ========
 * 1. 数据库读取流程:
 *    - 从数据库读取JSONB(PostgreSQL)或TEXT(SQLite)类型的值
 *    - 通过FromSql特性将JSON数据解析为KeyValue类型
 *    - 应用程序使用HashMap接口操作键值对数据
 *
 * 2. 数据库写入流程:
 *    - 应用程序创建或修改KeyValue实例
 *    - 通过ToSql特性将KeyValue序列化为JSON格式
 *    - 写入数据库的JSONB(PostgreSQL)或TEXT(SQLite)列
 *
 * 注意事项
 * ========
 * - 数据类型限制: 当前实现仅支持字符串值，不支持嵌套结构或其他数据类型
 * - 性能考虑: JSON序列化和反序列化可能影响性能，特别是对于大型键值集合
 * - 查询限制: 在SQLite中，无法直接查询JSON内部的键或值
 * - 数据一致性: 应用程序负责确保键值对数据的一致性和有效性
 * - 存储效率: 键值对存储可能不如专用列效率高，但提供了更大的灵活性
 */

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use diesel::backend::Backend;

use diesel::{deserialize, serialize};
#[cfg(feature = "postgres")]
use diesel::{pg::Pg, sql_types::Jsonb};
#[cfg(feature = "sqlite")]
use diesel::{sql_types::Text, sqlite::Sqlite};

#[derive(Debug, Clone, PartialEq, Eq, AsExpression, FromSqlRow)]
#[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
pub struct KeyValue(HashMap<String, String>);

impl KeyValue {
    pub fn new(m: HashMap<String, String>) -> Self {
        KeyValue(m)
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn into_hashmap(&self) -> HashMap<String, String> {
        self.0.clone()
    }
}

impl Deref for KeyValue {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KeyValue {
    fn deref_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.0
    }
}

#[cfg(feature = "postgres")]
impl deserialize::FromSql<Jsonb, Pg> for KeyValue {
    fn from_sql(value: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as deserialize::FromSql<Jsonb, Pg>>::from_sql(value)?;
        let kv: HashMap<String, String> = serde_json::from_value(value)?;
        Ok(KeyValue(kv))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Jsonb, Pg> for KeyValue {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(&self.0)?;
        <serde_json::Value as serialize::ToSql<Jsonb, Pg>>::to_sql(&value, &mut out.reborrow())
    }
}

#[cfg(feature = "sqlite")]
impl deserialize::FromSql<Text, Sqlite> for KeyValue
where
    *const str: deserialize::FromSql<Text, Sqlite>,
{
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s =
            <*const str as deserialize::FromSql<diesel::sql_types::Text, Sqlite>>::from_sql(value)?;
        let kv: HashMap<String, String> = serde_json::from_str(unsafe { &*s })?;
        Ok(KeyValue(kv))
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Text, Sqlite> for KeyValue {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(serialize::IsNull::No)
    }
}

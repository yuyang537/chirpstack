/*
 * 模块概述
 * ========
 * Measurements模块提供了一个用于管理和存储设备测量数据元数据的数据结构。该模块解决了
 * 在ChirpStack系统中定义、分类和持久化各种设备测量指标的需求，为应用程序提供了一种
 * 统一的方式来描述和处理不同类型的传感器数据和设备状态信息。
 * 
 * 该模块定义了Measurement结构体表示单个测量指标及其类型，MeasurementKind枚举定义了
 * 不同的测量值类型（如计数器、绝对值、仪表值、字符串等），以及Measurements结构体作为
 * HashMap<String, Measurement>的包装器。同时，为Measurements实现了Diesel ORM所需的
 * 序列化和反序列化特性，使其可以在PostgreSQL的JSONB类型和SQLite的TEXT类型之间进行映射。
 *
 * 文件功能
 * ========
 * 本文件(measurements.rs)实现了测量数据相关的类型及其特性，提供了以下主要功能：
 * 1. 定义Measurement结构体，表示单个测量指标及其类型
 * 2. 定义MeasurementKind枚举，分类不同类型的测量值
 * 3. 定义Measurements结构体，包装HashMap<String, Measurement>
 * 4. 提供创建和转换方法，便于与HashMap交互
 * 5. 实现Diesel ORM所需的序列化和反序列化特性
 *
 * 主要组件
 * ========
 * - Measurement结构体: 包含测量指标的名称和类型
 * - MeasurementKind枚举: 定义测量值的不同类型（UNKNOWN, COUNTER, ABSOLUTE, GAUGE, STRING）
 * - Measurements结构体: 包装HashMap<String, Measurement>类型
 * - new方法: 从HashMap创建Measurements实例
 * - into_hashmap方法: 将Measurements转换为HashMap
 * - Deref/DerefMut实现: 允许透明地访问底层HashMap的方法
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 *
 * 关键流程
 * ========
 * 1. 测量指标定义流程:
 *    - 应用程序定义设备可能上报的测量指标及其类型
 *    - 创建Measurement实例，指定名称和MeasurementKind
 *    - 将多个Measurement组织到Measurements集合中
 *
 * 2. 数据库交互流程:
 *    - 从数据库读取JSONB(PostgreSQL)或TEXT(SQLite)类型的值
 *    - 通过FromSql特性将JSON数据解析为Measurements类型
 *    - 应用程序使用Measurements管理和查询测量指标定义
 *    - 通过ToSql特性将Measurements序列化为JSON格式
 *    - 写入数据库的JSONB(PostgreSQL)或TEXT(SQLite)列
 *
 * 注意事项
 * ========
 * - 类型系统: MeasurementKind提供了基本的类型系统，但可能需要扩展以支持更复杂的数据类型
 * - 性能考虑: JSON序列化和反序列化可能影响性能，特别是对于大量测量指标
 * - 数据验证: 应用程序负责确保测量指标的名称和类型的一致性
 * - 扩展性: 未来可能需要扩展MeasurementKind以支持更多类型的测量值
 * - 数据库兼容性: JSON处理在不同数据库后端中可能有所不同
 */

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use diesel::backend::Backend;
use diesel::{deserialize, serialize};
#[cfg(feature = "postgres")]
use diesel::{pg::Pg, sql_types::Jsonb};
#[cfg(feature = "sqlite")]
use diesel::{sql_types::Text, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Measurement {
    pub name: String,
    pub kind: MeasurementKind,
}

#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    // Unknown.
    UNKNOWN,
    // Incrementing counters which are not reset on each reporting.
    COUNTER,
    // Counters that do get reset upon reading.
    ABSOLUTE,
    // E.g. a temperature value.
    GAUGE,
    // E.g. a firmware version, true / false value.
    STRING,
}

#[derive(Debug, Clone, AsExpression, FromSqlRow, PartialEq, Eq)]
#[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
pub struct Measurements(HashMap<String, Measurement>);

impl Measurements {
    pub fn new(m: HashMap<String, Measurement>) -> Self {
        Measurements(m)
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn into_hashmap(&self) -> HashMap<String, Measurement> {
        self.0.clone()
    }
}

impl Deref for Measurements {
    type Target = HashMap<String, Measurement>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Measurements {
    fn deref_mut(&mut self) -> &mut HashMap<String, Measurement> {
        &mut self.0
    }
}

#[cfg(feature = "postgres")]
impl deserialize::FromSql<Jsonb, Pg> for Measurements {
    fn from_sql(value: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as deserialize::FromSql<Jsonb, Pg>>::from_sql(value)?;
        let kv: HashMap<String, Measurement> = serde_json::from_value(value)?;
        Ok(Measurements::new(kv))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Jsonb, Pg> for Measurements {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Pg>) -> serialize::Result {
        let value = serde_json::to_value(&self.0)?;
        <serde_json::Value as serialize::ToSql<Jsonb, Pg>>::to_sql(&value, &mut out.reborrow())
    }
}

#[cfg(feature = "sqlite")]
impl deserialize::FromSql<Text, Sqlite> for Measurements
where
    *const str: deserialize::FromSql<Text, Sqlite>,
{
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s =
            <*const str as deserialize::FromSql<diesel::sql_types::Text, Sqlite>>::from_sql(value)?;
        let kv: HashMap<String, Measurement> = serde_json::from_str(unsafe { &*s })?;
        Ok(Measurements::new(kv))
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Text, Sqlite> for Measurements {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Sqlite>) -> serialize::Result {
        let value = serde_json::to_string(&self.0)?;
        out.set_value(value);
        Ok(serialize::IsNull::No)
    }
}

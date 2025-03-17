/*
 * 模块概述
 * ========
 * MulticastGroupSchedulingType模块定义了多播组调度类型的枚举及其数据库交互功能。该模块
 * 解决了在ChirpStack系统中为多播下行消息指定不同调度策略的需求，使网络服务器能够灵活地
 * 控制多播消息的发送时机，以适应不同的应用场景和设备能力。
 * 
 * 该模块定义了MulticastGroupSchedulingType枚举，包含两种调度类型：基于延迟的调度(DELAY)
 * 和基于GPS时间的调度(GPS_TIME)。同时，为该枚举实现了Diesel ORM所需的序列化和反序列化
 * 特性，使其可以在PostgreSQL和SQLite的TEXT类型中存储和检索。
 *
 * 文件功能
 * ========
 * 本文件(multicast_group_scheduling_type.rs)实现了多播组调度类型及其相关特性，提供了以下主要功能：
 * 1. 定义MulticastGroupSchedulingType枚举，表示不同的调度策略
 * 2. 实现Display特性，用于将枚举值转换为字符串
 * 3. 实现FromStr特性，用于从字符串解析枚举值
 * 4. 实现Diesel ORM所需的序列化和反序列化特性
 *
 * 主要组件
 * ========
 * - MulticastGroupSchedulingType枚举: 定义两种调度类型（DELAY和GPS_TIME）
 * - Display实现: 将枚举值转换为字符串表示
 * - FromStr实现: 从字符串解析枚举值
 * - FromSql实现: 从数据库TEXT类型读取枚举值
 * - ToSql实现: 将枚举值写入数据库TEXT类型
 *
 * 关键流程
 * ========
 * 1. 多播组调度类型选择流程:
 *    - 应用程序在创建或更新多播组时选择调度类型
 *    - 根据应用需求选择DELAY（基于延迟的简单调度）或GPS_TIME（基于精确GPS时间的调度）
 *    - 系统根据选择的调度类型应用相应的调度算法
 *
 * 2. 数据库交互流程:
 *    - 从数据库读取TEXT类型的值
 *    - 通过FromSql特性将文本值解析为MulticastGroupSchedulingType枚举
 *    - 应用程序使用枚举值确定调度策略
 *    - 通过ToSql特性将枚举值序列化为文本格式
 *    - 写入数据库的TEXT列
 *
 * 注意事项
 * ========
 * - 调度类型选择: DELAY适用于简单场景，而GPS_TIME适用于需要精确时间同步的场景
 * - 设备兼容性: 使用GPS_TIME调度要求设备支持GPS时间同步
 * - 数据库表示: 枚举值在数据库中以文本形式存储，便于查询和调试
 * - 扩展性: 未来可能需要添加更多调度类型以支持新的应用场景
 * - 向后兼容性: 添加新的枚举值时需要考虑现有数据的兼容性
 */

use std::fmt;
use std::str::FromStr;

use diesel::backend::Backend;
use diesel::sql_types::Text;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::{deserialize, serialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, AsExpression, FromSqlRow)]
#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum MulticastGroupSchedulingType {
    // Delay.
    DELAY,
    // GPS time.
    GPS_TIME,
}

impl fmt::Display for MulticastGroupSchedulingType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<DB> deserialize::FromSql<Text, DB> for MulticastGroupSchedulingType
where
    DB: Backend,
    *const str: deserialize::FromSql<Text, DB>,
{
    fn from_sql(value: <DB as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let string = <*const str>::from_sql(value)?;
        Ok(Self::from_str(unsafe { &*string })?)
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Text, diesel::pg::Pg> for MulticastGroupSchedulingType
where
    str: serialize::ToSql<Text, diesel::pg::Pg>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::pg::Pg>,
    ) -> serialize::Result {
        <str as serialize::ToSql<Text, diesel::pg::Pg>>::to_sql(
            &self.to_string(),
            &mut out.reborrow(),
        )
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Text, Sqlite> for MulticastGroupSchedulingType {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.to_string());
        Ok(serialize::IsNull::No)
    }
}

impl FromStr for MulticastGroupSchedulingType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "DELAY" => MulticastGroupSchedulingType::DELAY,
            "GPS_TIME" => MulticastGroupSchedulingType::GPS_TIME,
            _ => {
                return Err(anyhow!("Unexpected MulticastGroupSchedulingType: {}", s));
            }
        })
    }
}

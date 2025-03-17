/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的编解码器实现，用于处理设备上行和下行有效载荷的编码和解码。
 * 在LoRaWAN应用中，设备通常使用二进制格式传输数据以节省带宽，而应用程序则需要结构化数据。
 * 该模块提供了多种编解码器，将设备的二进制数据转换为结构化格式，以及将应用程序的结构化数据转换为设备可理解的二进制格式。
 *
 * 文件功能
 * ========
 * 该文件是codec模块的入口点，定义了编解码器的类型和通用接口。
 * 它提供了编解码器的枚举类型定义、序列化和反序列化功能，以及通用的编解码接口函数。
 * 主要功能包括定义支持的编解码器类型、提供二进制和结构化数据之间的转换接口，以及提供测量值提取功能。
 *
 * 主要组件
 * ========
 * - Codec枚举: 定义了支持的编解码器类型（NONE, CAYENNE_LPP, JS）
 * - binary_to_struct(): 将二进制数据转换为结构化数据
 * - struct_to_binary(): 将结构化数据转换为二进制数据
 * - get_measurements(): 从结构化数据中提取测量值
 * - 子模块: cayenne_lpp, js, convert等实现具体的编解码功能
 *
 * 关键流程
 * ========
 * 1. 上行数据处理流程:
 *    - 接收设备发送的二进制数据
 *    - 根据设备配置的编解码器类型选择适当的解码器
 *    - 调用binary_to_struct()将二进制数据转换为结构化数据
 *    - 提取测量值并传递给应用程序
 *
 * 2. 下行数据处理流程:
 *    - 接收应用程序发送的结构化数据
 *    - 根据设备配置的编解码器类型选择适当的编码器
 *    - 调用struct_to_binary()将结构化数据转换为二进制数据
 *    - 将二进制数据发送给设备
 *
 * 注意事项
 * ========
 * - 编解码器的选择应与设备的数据格式匹配，否则可能导致数据解释错误
 * - JavaScript编解码器提供了最大的灵活性，但也有执行时间限制
 * - 编解码过程中的错误应妥善处理，避免影响整个应用程序
 * - 自定义编解码器应考虑性能和安全性
 * - 在处理大量设备时，编解码器的性能可能成为瓶颈
 */

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use diesel::backend::Backend;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
use diesel::sql_types::Text;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::{deserialize, serialize};
use serde::{Deserialize, Serialize};

mod cayenne_lpp;
pub mod convert;
mod js;

#[derive(Deserialize, Serialize, Copy, Clone, Debug, Eq, PartialEq, AsExpression, FromSqlRow)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum Codec {
    NONE,
    CAYENNE_LPP,
    JS,
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<DB> deserialize::FromSql<Text, DB> for Codec
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
impl serialize::ToSql<Text, Pg> for Codec
where
    str: serialize::ToSql<Text, Pg>,
{
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Pg>) -> serialize::Result {
        <str as serialize::ToSql<Text, Pg>>::to_sql(&self.to_string(), &mut out.reborrow())
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Text, Sqlite> for Codec {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.to_string());
        Ok(serialize::IsNull::No)
    }
}

impl FromStr for Codec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "" | "NONE" => Codec::NONE,
            "CAYENNE_LPP" => Codec::CAYENNE_LPP,
            "JS" => Codec::JS,
            _ => {
                return Err(anyhow!("Unexpected codec: {}", s));
            }
        })
    }
}

pub async fn binary_to_struct(
    codec: Codec,
    recv_time: DateTime<Utc>,
    f_port: u8,
    variables: &HashMap<String, String>,
    decoder_config: &str,
    b: &[u8],
) -> Result<Option<pbjson_types::Struct>> {
    Ok(match codec {
        Codec::NONE => None,
        Codec::CAYENNE_LPP => Some(cayenne_lpp::decode(b).context("CayenneLpp decode")?),
        Codec::JS => Some(js::decode(recv_time, f_port, variables, decoder_config, b).await?),
    })
}

pub async fn struct_to_binary(
    codec: Codec,
    f_port: u8,
    variables: &HashMap<String, String>,
    encoder_config: &str,
    obj: &prost_types::Struct,
) -> Result<Vec<u8>> {
    Ok(match codec {
        Codec::NONE => Vec::new(),
        Codec::CAYENNE_LPP => cayenne_lpp::encode(obj).context("CayenneLpp encode")?,
        Codec::JS => js::encode(f_port, variables, encoder_config, obj).await?,
    })
}

pub fn get_measurements(s: &pbjson_types::Struct) -> HashMap<String, pbjson_types::value::Kind> {
    let mut out: HashMap<String, pbjson_types::value::Kind> = HashMap::new();

    for (k, v) in &s.fields {
        out.extend(_get_measurements(k, v));
    }

    out
}

fn _get_measurements(
    prefix: &str,
    v: &pbjson_types::Value,
) -> HashMap<String, pbjson_types::value::Kind> {
    let mut out: HashMap<String, pbjson_types::value::Kind> = HashMap::new();

    match &v.kind {
        None => {}
        Some(v) => match v {
            pbjson_types::value::Kind::NullValue(_) => {}
            pbjson_types::value::Kind::NumberValue(_)
            | pbjson_types::value::Kind::StringValue(_)
            | pbjson_types::value::Kind::BoolValue(_) => {
                out.insert(prefix.to_string(), v.clone());
            }
            pbjson_types::value::Kind::StructValue(v) => {
                for (k, v) in &v.fields {
                    out.extend(_get_measurements(&format!("{}_{}", prefix, k), v));
                }
            }
            pbjson_types::value::Kind::ListValue(v) => {
                for (i, v) in v.values.iter().enumerate() {
                    out.extend(_get_measurements(&format!("{}_{}", prefix, i), v));
                }
            }
        },
    }

    out
}

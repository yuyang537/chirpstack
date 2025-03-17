/*
 * 模块概述
 * ========
 * BigDecimal模块提供了一个高精度十进制数类型的包装器，用于在ChirpStack系统中存储和处理
 * 需要精确数值计算的数据。该模块解决了在数据库存储和应用程序处理之间无缝转换高精度数值
 * 的问题，特别是在处理传感器读数、坐标数据等需要精确表示的场景。
 * 
 * 该模块包装了第三方库bigdecimal的BigDecimal类型，并为其实现了Diesel ORM所需的序列化和
 * 反序列化特性，使其可以在PostgreSQL的Numeric类型和SQLite的Double类型之间进行映射。
 * 此外，还提供了各种便捷的转换方法，简化了应用代码中的数据处理。
 *
 * 文件功能
 * ========
 * 本文件(big_decimal.rs)实现了BigDecimal类型及其相关特性，提供了以下主要功能：
 * 1. 定义BigDecimal结构体，包装第三方库的bigdecimal::BigDecimal
 * 2. 实现与原始BigDecimal类型的转换方法
 * 3. 实现Diesel ORM所需的序列化和反序列化特性
 * 4. 提供便捷的操作方法，如Deref和DerefMut
 *
 * 主要组件
 * ========
 * - BigDecimal结构体: 包装bigdecimal::BigDecimal类型
 * - From/TryFrom实现: 提供与其他数值类型的转换
 * - Deref/DerefMut实现: 允许透明地访问底层BigDecimal的方法
 * - FromSql/ToSql实现: 支持PostgreSQL和SQLite的数据库序列化
 *
 * 关键流程
 * ========
 * 1. 数据库读取流程:
 *    - 从数据库读取Numeric(PostgreSQL)或Double(SQLite)类型的值
 *    - 通过FromSql特性转换为BigDecimal类型
 *    - 应用程序使用BigDecimal进行精确计算
 *
 * 2. 数据库写入流程:
 *    - 应用程序创建或修改BigDecimal值
 *    - 通过ToSql特性转换为数据库类型
 *    - 写入数据库的Numeric(PostgreSQL)或Double(SQLite)列
 *
 * 注意事项
 * ========
 * - 精度考虑: PostgreSQL的Numeric类型可以保持完整精度，而SQLite的Double可能导致精度损失
 * - 性能影响: 高精度数值操作可能比原始浮点数操作更消耗资源
 * - 数据库兼容性: 不同数据库后端对高精度数值的支持程度不同
 * - 转换限制: 从浮点数转换可能导致精度问题，应谨慎使用
 * - 内存使用: BigDecimal可能比原始数值类型使用更多内存
 */

use diesel::{
    backend::Backend,
    {deserialize, serialize},
};
#[cfg(feature = "postgres")]
use diesel::{pg::Pg, sql_types::Numeric};
#[cfg(feature = "sqlite")]
use diesel::{sql_types::Double, sqlite::Sqlite};

#[derive(Clone, Debug, Eq, PartialEq, AsExpression, FromSqlRow)]
#[cfg_attr(feature="postgres", diesel(sql_type = Numeric))]
#[cfg_attr(feature="sqlite", diesel(sql_type = Double))]
pub struct BigDecimal(bigdecimal::BigDecimal);

impl std::convert::AsRef<bigdecimal::BigDecimal> for BigDecimal {
    fn as_ref(&self) -> &bigdecimal::BigDecimal {
        &self.0
    }
}

impl std::convert::From<bigdecimal::BigDecimal> for BigDecimal {
    fn from(value: bigdecimal::BigDecimal) -> Self {
        Self(value)
    }
}

impl std::convert::TryFrom<f32> for BigDecimal {
    type Error = <bigdecimal::BigDecimal as TryFrom<f32>>::Error;
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        bigdecimal::BigDecimal::try_from(value).map(|bd| bd.into())
    }
}

impl std::ops::Deref for BigDecimal {
    type Target = bigdecimal::BigDecimal;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for BigDecimal {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "postgres")]
impl deserialize::FromSql<Numeric, Pg> for BigDecimal {
    fn from_sql(value: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let u = <bigdecimal::BigDecimal>::from_sql(value)?;
        Ok(BigDecimal(u))
    }
}

#[cfg(feature = "postgres")]
impl serialize::ToSql<Numeric, Pg> for BigDecimal {
    fn to_sql(&self, out: &mut serialize::Output<'_, '_, Pg>) -> serialize::Result {
        <bigdecimal::BigDecimal as serialize::ToSql<Numeric, Pg>>::to_sql(
            &self.0,
            &mut out.reborrow(),
        )
    }
}

#[cfg(feature = "sqlite")]
impl deserialize::FromSql<Double, Sqlite> for BigDecimal
where
    f64: deserialize::FromSql<Double, Sqlite>,
{
    fn from_sql(value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        use bigdecimal::FromPrimitive;
        let bd_val =
            <f64 as deserialize::FromSql<diesel::sql_types::Double, Sqlite>>::from_sql(value)?;
        let bd = bigdecimal::BigDecimal::from_f64(bd_val)
            .ok_or_else(|| format!("Unrepresentable BigDecimal from f64 value"))?;
        Ok(BigDecimal(bd))
    }
}

#[cfg(feature = "sqlite")]
impl serialize::ToSql<Double, Sqlite> for BigDecimal {
    fn to_sql<'b>(&self, out: &mut serialize::Output<'b, '_, Sqlite>) -> serialize::Result {
        use bigdecimal::ToPrimitive;
        let value = self
            .0
            .to_f64()
            .ok_or_else(|| format!("Unrepresentable f64 value as BigDecimal"))?;
        out.set_value(value);
        Ok(serialize::IsNull::No)
    }
}

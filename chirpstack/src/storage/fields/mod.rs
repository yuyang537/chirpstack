/*
 * 模块概述
 * ========
 * 字段模块是ChirpStack存储层的基础组件，提供了一系列自定义数据类型和类型转换功能，用于在
 * Rust数据类型和数据库存储类型之间进行映射。该模块解决了Diesel ORM在处理复杂数据类型时的
 * 局限性，特别是在支持PostgreSQL和SQLite两种不同数据库后端的情况下。
 * 
 * 字段模块中定义的类型通常包装了标准或第三方库中的类型，并为其实现了Diesel所需的序列化和
 * 反序列化特性，使这些类型可以无缝地与数据库交互。此外，这些类型还实现了各种便捷的转换和
 * 操作方法，简化了应用代码中的数据处理。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是字段模块的入口点，提供了以下主要功能：
 * 1. 导出各个子模块中定义的自定义类型
 * 2. 定义SQL类型别名，用于支持不同数据库后端
 * 3. 通过特性标志(feature flags)条件编译，支持PostgreSQL和SQLite
 *
 * 主要组件
 * ========
 * - BigDecimal: 高精度十进制数类型，用于存储精确的数值数据
 * - DevNonces: 设备随机数集合类型，用于防止重放攻击
 * - DeviceSession: 设备会话数据类型，存储设备通信状态
 * - KeyValue: 键值对映射类型，用于存储元数据和标签
 * - Measurements: 测量数据类型，用于存储设备传感器数据
 * - MulticastGroupSchedulingType: 多播组调度类型枚举
 * - Uuid: UUID类型，用于唯一标识符
 * - sql_types: SQL类型别名模块，根据数据库后端定义适当的类型
 *
 * 关键流程
 * ========
 * 1. 类型转换流程:
 *    - 应用代码使用Rust原生类型或第三方库类型
 *    - 字段模块的包装类型通过From/Into特性进行转换
 *    - Diesel ORM使用ToSql/FromSql特性进行数据库序列化/反序列化
 *    - 数据在应用和数据库之间无缝流动
 *
 * 2. 数据库兼容性处理:
 *    - 通过特性标志选择适当的数据库后端实现
 *    - 为不同数据库提供兼容的类型映射
 *    - 处理PostgreSQL和SQLite在类型系统上的差异
 *
 * 注意事项
 * ========
 * - 类型安全: 自定义类型提供了类型安全的数据处理
 * - 数据库兼容性: 支持PostgreSQL和SQLite两种数据库
 * - 序列化格式: 某些类型在不同数据库中使用不同的序列化格式
 * - 性能考虑: 复杂类型的序列化/反序列化可能影响性能
 * - 扩展性: 新增字段类型时需要实现完整的特性集
 */

mod big_decimal;
mod dev_nonces;
mod device_session;
mod key_value;
mod measurements;
mod multicast_group_scheduling_type;
mod uuid;

pub use big_decimal::BigDecimal;
pub use dev_nonces::DevNonces;
pub use device_session::DeviceSession;
pub use key_value::KeyValue;
pub use measurements::*;
pub use multicast_group_scheduling_type::MulticastGroupSchedulingType;
pub use uuid::Uuid;

#[cfg(feature = "postgres")]
pub mod sql_types {
    pub type Timestamptz = diesel::sql_types::Timestamptz;

    pub type JsonT = diesel::sql_types::Jsonb;

    pub type Uuid = diesel::sql_types::Uuid;
}

#[cfg(feature = "sqlite")]
pub mod sql_types {
    pub type Timestamptz = diesel::sql_types::TimestamptzSqlite;

    // TODO: sqlite is adding "jsonb" support, different from postgres
    // So we may switch the column to blob?
    // see https://sqlite.org/draft/jsonb.html
    pub type JsonT = diesel::sql_types::Text;

    // Sqlite has no native json type so use text
    pub type Uuid = diesel::sql_types::Text;
}

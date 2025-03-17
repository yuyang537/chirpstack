/*
 * 模块概述
 * ========
 * 存储模块是ChirpStack LoRaWAN网络服务器的核心组件之一，负责管理所有持久化数据的存储和检索。
 * 该模块提供了一个抽象层，使应用程序能够与不同的数据库后端（如PostgreSQL和SQLite）进行交互，
 * 同时也管理与Redis缓存的连接，用于临时数据存储和消息队列。
 * 
 * 存储模块采用了分层设计，将不同类型的数据（如设备、应用、租户等）分离到各自的子模块中，
 * 每个子模块负责特定类型数据的CRUD（创建、读取、更新、删除）操作。这种设计使代码更加模块化，
 * 便于维护和扩展。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是存储模块的入口点，提供了以下主要功能：
 * 1. 定义和导出存储模块的子模块结构
 * 2. 提供数据库连接池和Redis连接池的初始化和管理
 * 3. 实现数据库迁移功能，确保数据库结构与应用程序版本匹配
 * 4. 提供Redis键前缀管理，支持多租户隔离
 * 5. 提供用于测试的数据库和Redis重置功能
 *
 * 主要组件
 * ========
 * - AsyncRedisPool: Redis连接池的抽象，支持单节点和集群模式
 * - AsyncRedisPoolConnection: Redis连接的抽象，实现了ConnectionLike特性
 * - setup(): 初始化存储模块，包括数据库和Redis连接
 * - get_async_redis_conn(): 获取Redis连接
 * - run_db_migrations(): 运行数据库迁移脚本
 * - redis_key(): 生成带有前缀的Redis键
 * - reset_db(): 重置数据库（用于测试）
 * - reset_redis(): 重置Redis数据（用于测试）
 *
 * 关键流程
 * ========
 * 1. 存储模块初始化流程:
 *    - 根据配置创建数据库连接池
 *    - 根据配置创建Redis连接池
 *    - 运行数据库迁移脚本，确保数据库结构最新
 *    - 初始化Prometheus监控指标
 *
 * 2. 数据访问流程:
 *    - 从连接池获取数据库或Redis连接
 *    - 执行查询或命令
 *    - 处理结果或错误
 *    - 连接自动返回池中
 *
 * 注意事项
 * ========
 * - 数据库抽象: 模块支持PostgreSQL和SQLite，通过特性标志选择
 * - 连接池管理: 使用连接池提高性能，避免频繁创建连接
 * - 事务处理: 确保数据一致性，特别是在涉及多个表的操作中
 * - 错误处理: 统一的错误类型和处理机制
 * - 性能监控: 集成Prometheus指标，监控数据库操作性能
 * - 并发安全: 使用RwLock保护共享资源，确保线程安全
 */

use std::sync::RwLock;
use std::time::Instant;

use anyhow::Result;
use diesel_async::async_connection_wrapper::AsyncConnectionWrapper;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use redis::aio::ConnectionLike;
use tokio::sync::RwLock as TokioRwLock;
use tokio::task;
use tracing::{error, info};

use crate::config;

pub mod api_key;
pub mod application;
pub mod device;
pub mod device_gateway;
pub mod device_keys;
pub mod device_profile;
pub mod device_profile_template;
pub mod device_queue;
pub mod device_session;
pub mod downlink_frame;
pub mod error;
pub mod fields;
pub mod gateway;
pub mod helpers;
pub mod mac_command;
pub mod metrics;
pub mod multicast;
pub mod passive_roaming;
#[cfg(feature = "postgres")]
mod postgres;
pub mod relay;
pub mod schema;
#[cfg(feature = "postgres")]
mod schema_postgres;
#[cfg(feature = "sqlite")]
mod schema_sqlite;
pub mod search;
#[cfg(feature = "sqlite")]
mod sqlite;
pub mod tenant;
pub mod user;

use crate::monitoring::prometheus;

lazy_static! {
    static ref ASYNC_REDIS_POOL: TokioRwLock<Option<AsyncRedisPool>> = TokioRwLock::new(None);
    static ref REDIS_PREFIX: RwLock<String> = RwLock::new("".to_string());
    static ref STORAGE_REDIS_CONN_GET: Histogram = {
        let histogram = Histogram::new(exponential_buckets(0.001, 2.0, 12));
        prometheus::register(
            "storage_redis_conn_get_duration_seconds",
            "Time between requesting a Redis connection and the connection-pool returning it",
            histogram.clone(),
        );
        histogram
    };
}

#[cfg(feature = "postgres")]
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations_postgres");
#[cfg(feature = "sqlite")]
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations_sqlite");

#[cfg(feature = "postgres")]
pub use postgres::{
    db_transaction, get_async_db_conn, AsyncPgPoolConnection as AsyncDbPoolConnection,
};
#[cfg(feature = "sqlite")]
pub use sqlite::{
    db_transaction, get_async_db_conn, AsyncSqlitePoolConnection as AsyncDbPoolConnection,
};

#[derive(Clone)]
pub enum AsyncRedisPool {
    Client(deadpool_redis::Pool),
    ClusterClient(deadpool_redis::cluster::Pool),
}

pub enum AsyncRedisPoolConnection {
    Client(deadpool_redis::Connection),
    ClusterClient(deadpool_redis::cluster::Connection),
}

impl ConnectionLike for AsyncRedisPoolConnection {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            AsyncRedisPoolConnection::Client(v) => v.req_packed_command(cmd),
            AsyncRedisPoolConnection::ClusterClient(v) => v.req_packed_command(cmd),
        }
    }
    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            AsyncRedisPoolConnection::Client(v) => v.req_packed_commands(cmd, offset, count),
            AsyncRedisPoolConnection::ClusterClient(v) => v.req_packed_commands(cmd, offset, count),
        }
    }
    fn get_db(&self) -> i64 {
        match self {
            AsyncRedisPoolConnection::Client(v) => v.get_db(),
            AsyncRedisPoolConnection::ClusterClient(v) => v.get_db(),
        }
    }
}

pub async fn setup() -> Result<()> {
    let conf = config::get();

    #[cfg(feature = "postgres")]
    {
        postgres::setup(&conf.postgresql)?;
    }
    #[cfg(feature = "sqlite")]
    {
        sqlite::setup(&conf.sqlite)?;
    }
    run_db_migrations().await?;

    info!("Setting up Redis client");
    if conf.redis.cluster {
        let pool = deadpool_redis::cluster::Config::from_urls(conf.redis.servers.clone())
            .builder()?
            .max_size(conf.redis.max_open_connections as usize)
            .build()?;
        set_async_redis_pool(AsyncRedisPool::ClusterClient(pool)).await;
    } else {
        let pool = deadpool_redis::Config::from_url(conf.redis.servers[0].clone())
            .builder()?
            .max_size(conf.redis.max_open_connections as usize)
            .build()?;
        set_async_redis_pool(AsyncRedisPool::Client(pool)).await;
    }

    if !conf.redis.key_prefix.is_empty() {
        info!(prefix = %conf.redis.key_prefix, "Setting Redis prefix");
        REDIS_PREFIX
            .write()
            .unwrap()
            .clone_from(&conf.redis.key_prefix);
    }

    Ok(())
}

async fn get_async_redis_pool() -> Result<AsyncRedisPool> {
    let pool_r = ASYNC_REDIS_POOL.read().await;
    let pool: AsyncRedisPool = pool_r
        .as_ref()
        .ok_or_else(|| anyhow!("Redis connection pool is not initialized"))?
        .clone();
    Ok(pool)
}

pub async fn get_async_redis_conn() -> Result<AsyncRedisPoolConnection> {
    let pool = get_async_redis_pool().await?;

    let start = Instant::now();
    let res = match pool {
        AsyncRedisPool::Client(v) => AsyncRedisPoolConnection::Client(v.get().await?),
        AsyncRedisPool::ClusterClient(v) => {
            AsyncRedisPoolConnection::ClusterClient(v.clone().get().await?)
        }
    };

    STORAGE_REDIS_CONN_GET.observe(start.elapsed().as_secs_f64());

    Ok(res)
}

pub async fn run_db_migrations() -> Result<()> {
    info!("Applying schema migrations");

    let c = get_async_db_conn().await?;
    let mut c_wrapped: AsyncConnectionWrapper<AsyncDbPoolConnection> =
        AsyncConnectionWrapper::from(c);

    task::spawn_blocking(move || -> Result<()> {
        c_wrapped
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("{}", e))?;

        Ok(())
    })
    .await?
}

async fn set_async_redis_pool(p: AsyncRedisPool) {
    let mut pool_w = ASYNC_REDIS_POOL.write().await;
    *pool_w = Some(p);
}

pub fn redis_key(s: String) -> String {
    let prefix = REDIS_PREFIX.read().unwrap();
    format!("{}{}", prefix, s)
}

#[cfg(test)]
pub async fn reset_db() -> Result<()> {
    let c = get_async_db_conn().await?;
    let mut c_wrapped: AsyncConnectionWrapper<AsyncDbPoolConnection> =
        AsyncConnectionWrapper::from(c);

    tokio::task::spawn_blocking(move || -> Result<()> {
        c_wrapped
            .revert_all_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("During revert: {}", e))?;
        c_wrapped
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("During run: {}", e))?;

        Ok(())
    })
    .await?
}

#[cfg(test)]
pub async fn reset_redis() -> Result<()> {
    () = redis::cmd("FLUSHDB")
        .query_async(&mut get_async_redis_conn().await?)
        .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_prefix_no_prefix() {
        *REDIS_PREFIX.write().unwrap() = "".to_string();
        assert_eq!("lora:test:key", redis_key("lora:test:key".to_string()));
    }

    #[test]
    fn test_prefix() {
        *REDIS_PREFIX.write().unwrap() = "foobar:".to_string();
        assert_eq!(
            "foobar:lora:test:key",
            redis_key("lora:test:key".to_string())
        );
    }
}

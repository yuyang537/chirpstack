/*
 * 模块概述
 * ========
 * SQLite模块是ChirpStack存储层的数据库后端实现之一，负责管理与SQLite数据库的连接和交互。
 * 该模块提供了连接池管理、事务处理和性能监控等功能，使应用程序能够高效、可靠地访问SQLite
 * 数据库。
 * 
 * 与PostgreSQL模块类似，SQLite模块也采用了异步编程模型，但由于SQLite本身是一个嵌入式数据库，
 * 不支持原生的异步操作，因此该模块使用了SyncConnectionWrapper来将同步的SQLite连接包装为
 * 异步接口，以便与ChirpStack的异步架构集成。
 *
 * 文件功能
 * ========
 * 本文件(sqlite.rs)实现了SQLite数据库的连接和管理功能，提供了以下主要功能：
 * 1. 初始化和配置SQLite连接池
 * 2. 提供获取数据库连接的接口
 * 3. 实现数据库事务处理
 * 4. 集成Prometheus监控，跟踪数据库连接性能
 *
 * 主要组件
 * ========
 * - AsyncSqlitePool: SQLite连接池类型
 * - AsyncSqlitePoolConnection: SQLite连接类型
 * - setup(): 初始化SQLite连接池
 * - get_async_db_conn(): 获取数据库连接
 * - db_transaction(): 执行数据库事务
 * - sqlite_establish_connection(): 建立数据库连接的自定义函数
 *
 * 关键流程
 * ========
 * 1. 连接池初始化流程:
 *    - 读取SQLite配置
 *    - 配置连接管理器
 *    - 创建连接池，设置最大连接数
 *    - 存储连接池以供后续使用
 *
 * 2. 数据库连接获取流程:
 *    - 从连接池请求连接
 *    - 记录获取连接的时间（用于监控）
 *    - 返回连接或报告错误
 *
 * 3. 事务处理流程:
 *    - 开始事务
 *    - 执行回调函数中的数据库操作
 *    - 根据操作结果提交或回滚事务
 *
 * 注意事项
 * ========
 * - 连接池管理: 合理配置连接池大小，避免资源耗尽
 * - 错误处理: 妥善处理数据库连接和查询错误
 * - 性能监控: 使用Prometheus指标监控数据库性能
 * - 同步包装: 使用SyncConnectionWrapper将同步SQLite操作包装为异步接口
 * - 文件锁定: 注意SQLite的文件锁定机制，特别是在多进程环境中
 * - 事务隔离: 确保事务的ACID属性，特别是在并发访问场景下
 */

use std::sync::RwLock;
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::monitoring::prometheus;
use diesel::sqlite::SqliteConnection;
use diesel::{Connection, ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::deadpool::{Object as DeadpoolObject, Pool as DeadpoolPool};
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;
use futures::future::{BoxFuture, FutureExt, TryFutureExt};
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use scoped_futures::ScopedBoxFuture;

use crate::config;

pub type AsyncSqlitePool = DeadpoolPool<SyncConnectionWrapper<SqliteConnection>>;
pub type AsyncSqlitePoolConnection = DeadpoolObject<SyncConnectionWrapper<SqliteConnection>>;

lazy_static! {
    static ref ASYNC_SQLITE_POOL: RwLock<Option<AsyncSqlitePool>> = RwLock::new(None);
    static ref STORAGE_SQLITE_CONN_GET: Histogram = {
        let histogram = Histogram::new(exponential_buckets(0.001, 2.0, 12));
        prometheus::register(
            "storage_sqlite_conn_get_duration_seconds",
            "Time between requesting a SQLite connection and the connection-pool returning it",
            histogram.clone(),
        );
        histogram
    };
}

pub fn setup(conf: &config::Sqlite) -> Result<()> {
    info!("Setting up SQLite connection pool");
    let mut config = ManagerConfig::default();
    config.custom_setup = Box::new(sqlite_establish_connection);
    let mgr =
        AsyncDieselConnectionManager::<SyncConnectionWrapper<SqliteConnection>>::new_with_config(
            &conf.path, config,
        );
    let pool = DeadpoolPool::builder(mgr)
        .max_size(conf.max_open_connections as usize)
        .build()?;
    set_async_db_pool(pool);

    Ok(())
}

fn sqlite_establish_connection(
    url: &str,
) -> BoxFuture<ConnectionResult<SyncConnectionWrapper<SqliteConnection>>> {
    let url = url.to_string();
    tokio::task::spawn_blocking(
        move || -> ConnectionResult<SyncConnectionWrapper<SqliteConnection>> {
            let mut conn = SqliteConnection::establish(&url)?;

            use diesel::connection::SimpleConnection;
            let conf = config::get();
            let pragmas = &conf
                .sqlite
                .pragmas
                .iter()
                .map(|p| format!("PRAGMA {};", p))
                .collect::<Vec<String>>()
                .join("");
            conn.batch_execute(&pragmas)
                .map_err(|err| ConnectionError::BadConnection(err.to_string()))?;
            Ok(SyncConnectionWrapper::new(conn))
        },
    )
    .unwrap_or_else(|err| Err(ConnectionError::BadConnection(err.to_string())))
    .boxed()
}

fn get_async_db_pool() -> Result<AsyncSqlitePool> {
    let pool_r = ASYNC_SQLITE_POOL.read().unwrap();
    let pool: AsyncSqlitePool = pool_r
        .as_ref()
        .ok_or_else(|| anyhow!("SQLite connection pool is not initialized"))?
        .clone();
    Ok(pool)
}

pub async fn get_async_db_conn() -> Result<AsyncSqlitePoolConnection> {
    let pool = get_async_db_pool()?;

    let start = Instant::now();
    let res = pool.get().await?;

    STORAGE_SQLITE_CONN_GET.observe(start.elapsed().as_secs_f64());

    Ok(res)
}

pub async fn db_transaction<'a, R, E, F>(
    conn: &mut AsyncSqlitePoolConnection,
    callback: F,
) -> Result<R, E>
where
    F: for<'r> FnOnce(
            &'r mut SyncConnectionWrapper<SqliteConnection>,
        ) -> ScopedBoxFuture<'a, 'r, Result<R, E>>
        + Send
        + 'a,
    E: From<diesel::result::Error> + Send + 'a,
    R: Send + 'a,
{
    conn.immediate_transaction(callback).await
}

fn set_async_db_pool(p: AsyncSqlitePool) {
    let mut pool_w = ASYNC_SQLITE_POOL.write().unwrap();
    *pool_w = Some(p);
}

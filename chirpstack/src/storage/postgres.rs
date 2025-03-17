/*
 * 模块概述
 * ========
 * PostgreSQL模块是ChirpStack存储层的数据库后端实现之一，负责管理与PostgreSQL数据库的连接
 * 和交互。该模块提供了连接池管理、事务处理和性能监控等功能，使应用程序能够高效、可靠地
 * 访问PostgreSQL数据库。
 * 
 * 该模块采用了异步编程模型，基于diesel_async库实现异步数据库操作，以提高系统的并发处理
 * 能力和资源利用率。通过连接池技术，模块可以有效管理数据库连接，避免频繁创建和关闭连接
 * 带来的性能开销。
 *
 * 文件功能
 * ========
 * 本文件(postgres.rs)实现了PostgreSQL数据库的连接和管理功能，提供了以下主要功能：
 * 1. 初始化和配置PostgreSQL连接池
 * 2. 提供获取数据库连接的接口
 * 3. 实现数据库事务处理
 * 4. 集成Prometheus监控，跟踪数据库连接性能
 * 5. 支持TLS加密连接
 *
 * 主要组件
 * ========
 * - AsyncPgPool: PostgreSQL连接池类型
 * - AsyncPgPoolConnection: PostgreSQL连接类型
 * - setup(): 初始化PostgreSQL连接池
 * - get_async_db_conn(): 获取数据库连接
 * - db_transaction(): 执行数据库事务
 * - pg_establish_connection(): 建立数据库连接的自定义函数，支持TLS
 *
 * 关键流程
 * ========
 * 1. 连接池初始化流程:
 *    - 读取PostgreSQL配置
 *    - 配置连接管理器，包括TLS设置
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
 * - 安全性: 支持TLS加密连接，保护数据传输安全
 * - 异步编程: 使用异步接口提高并发性能
 * - 事务隔离: 确保事务的ACID属性
 */

use std::sync::RwLock;
use std::time::Instant;

use anyhow::Result;
use tracing::{error, info};

use crate::monitoring::prometheus;
use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::deadpool::{Object as DeadpoolObject, Pool as DeadpoolPool};
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::{AsyncConnection, AsyncPgConnection};
use futures::{future::BoxFuture, FutureExt};
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use scoped_futures::ScopedBoxFuture;

use crate::config;

use crate::helpers::tls::get_root_certs;

pub type AsyncPgPool = DeadpoolPool<AsyncPgConnection>;
pub type AsyncPgPoolConnection = DeadpoolObject<AsyncPgConnection>;

lazy_static! {
    static ref ASYNC_PG_POOL: RwLock<Option<AsyncPgPool>> = RwLock::new(None);
    static ref STORAGE_PG_CONN_GET: Histogram = {
        let histogram = Histogram::new(exponential_buckets(0.001, 2.0, 12));
        prometheus::register(
            "storage_pg_conn_get_duration_seconds",
            "Time between requesting a PostgreSQL connection and the connection-pool returning it",
            histogram.clone(),
        );
        histogram
    };
}

pub fn setup(conf: &config::Postgresql) -> Result<()> {
    info!("Setting up PostgreSQL connection pool");
    let mut config = ManagerConfig::default();
    config.custom_setup = Box::new(pg_establish_connection);
    let mgr = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(&conf.dsn, config);
    let pool = DeadpoolPool::builder(mgr)
        .max_size(conf.max_open_connections as usize)
        .build()?;
    set_async_db_pool(pool);

    Ok(())
}

// Source:
// https://github.com/weiznich/diesel_async/blob/main/examples/postgres/pooled-with-rustls/src/main.rs
fn pg_establish_connection(config: &str) -> BoxFuture<ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        let conf = config::get();

        let root_certs = get_root_certs(if conf.postgresql.ca_cert.is_empty() {
            None
        } else {
            Some(conf.postgresql.ca_cert.clone())
        })
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        let rustls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_certs)
            .with_no_client_auth();
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
        let (client, conn) = tokio_postgres::connect(config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                error!(error = %e, "PostgreSQL connection error");
            }
        });
        AsyncPgConnection::try_from(client).await
    };
    fut.boxed()
}

fn get_async_db_pool() -> Result<AsyncPgPool> {
    let pool_r = ASYNC_PG_POOL.read().unwrap();
    let pool: AsyncPgPool = pool_r
        .as_ref()
        .ok_or_else(|| anyhow!("PostgreSQL connection pool is not initialized"))?
        .clone();
    Ok(pool)
}

pub async fn get_async_db_conn() -> Result<AsyncPgPoolConnection> {
    let pool = get_async_db_pool()?;

    let start = Instant::now();
    let res = pool.get().await?;

    STORAGE_PG_CONN_GET.observe(start.elapsed().as_secs_f64());

    Ok(res)
}

pub async fn db_transaction<'a, R, E, F>(
    conn: &mut AsyncPgPoolConnection,
    callback: F,
) -> Result<R, E>
where
    F: for<'r> FnOnce(&'r mut AsyncPgPoolConnection) -> ScopedBoxFuture<'a, 'r, Result<R, E>>
        + Send
        + 'a,
    E: From<diesel::result::Error> + Send + 'a,
    R: Send + 'a,
{
    conn.transaction(callback).await
}

fn set_async_db_pool(p: AsyncPgPool) {
    let mut pool_w = ASYNC_PG_POOL.write().unwrap();
    *pool_w = Some(p);
}

/*
 * 模块概述
 * ========
 * api_key模块提供了用于管理ChirpStack API密钥的存储和检索功能。API密钥是ChirpStack
 * 安全架构的重要组成部分，用于验证和授权对REST API的访问请求。该模块支持两种类型的
 * API密钥：全局管理员密钥和租户级别密钥，分别用于系统级管理和特定租户资源的访问控制。
 * 
 * 该模块使用关系型数据库(PostgreSQL或SQLite)作为存储后端，提供了完整的CRUD操作和
 * 查询功能。API密钥通过UUID进行唯一标识，并可选择性地与租户关联，支持多租户环境下的
 * 访问控制管理。
 *
 * 文件功能
 * ========
 * 本文件(api_key.rs)实现了API密钥的存储和管理功能，提供了以下主要功能：
 * 1. 创建、读取和删除API密钥
 * 2. 列出和筛选API密钥
 * 3. 支持按租户ID和管理员状态过滤API密钥
 * 4. 验证API密钥的有效性
 *
 * 主要组件
 * ========
 * - ApiKey结构体: 定义API密钥数据模型，包含基本信息
 * - Filters结构体: 定义API密钥查询过滤条件
 * - create函数: 创建新的API密钥
 * - delete函数: 删除现有API密钥
 * - get_count函数: 获取符合过滤条件的API密钥数量
 * - list函数: 列出符合过滤条件的API密钥
 * - validate方法: 验证API密钥参数的有效性
 *
 * 关键流程
 * ========
 * 1. API密钥创建流程:
 *    - 验证API密钥参数的有效性
 *    - 将API密钥插入数据库
 *    - 返回创建的API密钥，包括生成的UUID
 *
 * 2. API密钥查询流程:
 *    - 根据过滤条件构建查询
 *    - 应用分页参数(limit和offset)
 *    - 执行查询并返回结果列表
 *
 * 3. API密钥删除流程:
 *    - 根据UUID删除API密钥
 *    - 验证删除操作是否成功
 *    - 记录删除操作的日志
 *
 * 注意事项
 * ========
 * - 安全性: API密钥是敏感信息，需要妥善保护
 * - 权限控制: 管理员API密钥具有系统级权限，租户API密钥仅限于特定租户资源
 * - 多租户支持: API密钥可以与租户关联，确保租户隔离
 * - 验证逻辑: 确保API密钥的名称不为空
 * - 性能考虑: API密钥查询操作的性能对系统响应时间有影响
 * - 测试支持: 包含测试模块，用于验证API密钥管理功能
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::dsl;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use tracing::info;
use uuid::Uuid;

use super::error::Error;
use super::schema::api_key;
use super::{error, fields, get_async_db_conn};

#[derive(Queryable, Insertable, PartialEq, Eq, Debug)]
#[diesel(table_name = api_key)]
pub struct ApiKey {
    pub id: fields::Uuid,
    pub created_at: DateTime<Utc>,
    pub name: String,
    pub is_admin: bool,
    pub tenant_id: Option<fields::Uuid>,
}

impl ApiKey {
    fn validate(&self) -> Result<(), Error> {
        if self.name.is_empty() {
            return Err(Error::Validation("name is not set".into()));
        }

        Ok(())
    }
}

impl Default for ApiKey {
    fn default() -> Self {
        ApiKey {
            id: Uuid::new_v4().into(),
            created_at: Utc::now(),
            name: "".into(),
            is_admin: false,
            tenant_id: None,
        }
    }
}

#[derive(Default, Clone)]
pub struct Filters {
    pub tenant_id: Option<Uuid>,
    pub is_admin: bool,
}

pub async fn create(ak: ApiKey) -> Result<ApiKey, Error> {
    ak.validate()?;

    let ak: ApiKey = diesel::insert_into(api_key::table)
        .values(&ak)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| error::Error::from_diesel(e, ak.id.to_string()))?;
    info!(id = %ak.id, "Api-key created");
    Ok(ak)
}

pub async fn delete(id: &Uuid) -> Result<(), Error> {
    let ra = diesel::delete(api_key::dsl::api_key.find(fields::Uuid::from(id)))
        .execute(&mut get_async_db_conn().await?)
        .await?;
    if ra == 0 {
        return Err(Error::NotFound(id.to_string()));
    }
    info!(id = %id, "Api-key deleted");
    Ok(())
}

pub async fn get_count(filters: &Filters) -> Result<i64, Error> {
    let mut q = api_key::dsl::api_key
        .select(dsl::count_star())
        .filter(api_key::dsl::is_admin.eq(filters.is_admin))
        .into_boxed();

    if let Some(tenant_id) = &filters.tenant_id {
        q = q.filter(api_key::dsl::tenant_id.eq(fields::Uuid::from(tenant_id)));
    }

    Ok(q.first(&mut get_async_db_conn().await?).await?)
}

pub async fn list(limit: i64, offset: i64, filters: &Filters) -> Result<Vec<ApiKey>, Error> {
    let mut q = api_key::dsl::api_key
        .filter(api_key::dsl::is_admin.eq(filters.is_admin))
        .into_boxed();

    if let Some(tenant_id) = &filters.tenant_id {
        q = q.filter(api_key::dsl::tenant_id.eq(fields::Uuid::from(tenant_id)));
    }

    let items = q
        .order_by(api_key::dsl::name)
        .limit(limit)
        .offset(offset)
        .load(&mut get_async_db_conn().await?)
        .await?;
    Ok(items)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::storage::tenant;
    use crate::test;

    struct FilterTest<'a> {
        filters: Filters,
        keys: Vec<&'a ApiKey>,
        count: usize,
        limit: i64,
        offset: i64,
    }

    pub async fn get(id: &Uuid) -> Result<ApiKey, Error> {
        api_key::dsl::api_key
            .find(fields::Uuid::from(id))
            .first(&mut get_async_db_conn().await?)
            .await
            .map_err(|e| error::Error::from_diesel(e, id.to_string()))
    }

    pub async fn create_api_key(is_admin: bool, is_tenant: bool) -> ApiKey {
        let ak = ApiKey {
            name: "test api key".into(),
            is_admin,
            tenant_id: match is_tenant {
                false => None,
                true => Some(tenant::test::create_tenant().await.id),
            },
            ..Default::default()
        };

        create(ak).await.unwrap()
    }

    #[tokio::test]
    async fn api_key() {
        let _guard = test::prepare().await;
        let ak_admin = create_api_key(true, false).await;
        let ak_tenant = create_api_key(false, true).await;

        // get
        let ak_get = get(&ak_admin.id).await.unwrap();
        assert_eq!(ak_admin, ak_get);

        // get count and list
        let tests = vec![
            FilterTest {
                filters: Filters {
                    tenant_id: None,
                    is_admin: true,
                },
                keys: vec![&ak_admin],
                count: 1,
                limit: 10,
                offset: 0,
            },
            FilterTest {
                filters: Filters {
                    tenant_id: ak_tenant.tenant_id.map(|u| u.into()),
                    is_admin: false,
                },
                keys: vec![&ak_tenant],
                count: 1,
                limit: 10,
                offset: 0,
            },
        ];

        for tst in tests {
            let count = get_count(&tst.filters).await.unwrap() as usize;
            assert_eq!(tst.count, count);

            let items = list(tst.limit, tst.offset, &tst.filters).await.unwrap();
            assert_eq!(
                tst.keys
                    .iter()
                    .map(|k| { k.id.to_string() })
                    .collect::<String>(),
                items
                    .iter()
                    .map(|k| { k.id.to_string() })
                    .collect::<String>()
            );
        }

        // delete
        delete(&ak_admin.id).await.unwrap();
        assert!(delete(&ak_admin.id).await.is_err());
    }
}

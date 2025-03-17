/*
 * 模块概述
 * ========
 * 租户模块是ChirpStack多租户架构的核心组件，负责管理系统中的租户及其用户关系。在ChirpStack中，
 * 租户是资源隔离的基本单位，每个租户可以拥有自己的应用、设备、网关等资源，并且可以独立管理这些
 * 资源的访问权限。
 * 
 * 该模块提供了租户的创建、查询、更新和删除功能，以及租户与用户之间关系的管理。通过这些功能，
 * ChirpStack可以支持多租户部署，满足不同组织或部门对LoRaWAN网络服务器的隔离需求。
 *
 * 文件功能
 * ========
 * 本文件(tenant.rs)实现了租户相关的数据库操作，提供了以下主要功能：
 * 1. 租户的CRUD（创建、读取、更新、删除）操作
 * 2. 租户用户关系的管理（添加、更新、删除用户与租户的关联）
 * 3. 租户列表查询和过滤
 * 4. 租户用户列表查询
 *
 * 主要组件
 * ========
 * - Tenant结构体: 表示租户的基本信息，包括ID、名称、描述、资源限制等
 * - TenantUser结构体: 表示租户与用户的关联关系，包括权限设置
 * - TenantUserListItem结构体: 用于列表展示的租户用户信息
 * - Filters结构体: 用于租户列表查询的过滤条件
 * - create(): 创建新租户
 * - get(): 获取租户信息
 * - update(): 更新租户信息
 * - delete(): 删除租户
 * - add_user(): 将用户添加到租户
 * - update_user(): 更新租户用户关系
 * - delete_user(): 删除租户用户关系
 * - get_users(): 获取租户的用户列表
 *
 * 关键流程
 * ========
 * 1. 租户管理流程:
 *    - 创建租户，设置基本信息和资源限制
 *    - 查询和更新租户信息
 *    - 删除租户（同时会删除关联的资源）
 *
 * 2. 租户用户管理流程:
 *    - 将用户添加到租户，设置用户权限
 *    - 更新用户在租户中的权限
 *    - 查询租户中的用户列表
 *    - 从租户中删除用户
 *
 * 3. 租户资源限制:
 *    - 设置租户可以拥有的最大设备数量
 *    - 设置租户可以拥有的最大网关数量
 *    - 控制租户的网关访问权限（公共/私有）
 *
 * 注意事项
 * ========
 * - 多租户隔离: 确保不同租户之间的资源隔离，防止跨租户访问
 * - 权限控制: 租户内用户权限的精细化控制，包括管理员、设备管理员和网关管理员
 * - 资源限制: 通过租户配置控制资源使用上限，防止资源滥用
 * - 数据一致性: 删除租户时需要处理关联资源，确保数据一致性
 * - 性能考虑: 租户查询是高频操作，需要注意查询性能
 */

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use tracing::info;
use uuid::Uuid;

use super::error::Error;
use super::schema::{tenant, tenant_user, user};
use super::{fields, get_async_db_conn};

#[derive(Queryable, Insertable, PartialEq, Eq, Debug, Clone)]
#[diesel(table_name = tenant)]
pub struct Tenant {
    pub id: fields::Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub name: String,
    pub description: String,
    pub can_have_gateways: bool,
    pub max_device_count: i32,
    pub max_gateway_count: i32,
    pub private_gateways_up: bool,
    pub private_gateways_down: bool,
    pub tags: fields::KeyValue,
}

impl Tenant {
    fn validate(&self) -> Result<(), Error> {
        if self.name.is_empty() {
            return Err(Error::Validation("name is not set".into()));
        }
        Ok(())
    }
}

impl Default for Tenant {
    fn default() -> Self {
        let now = Utc::now();

        Tenant {
            id: Uuid::new_v4().into(),
            created_at: now,
            updated_at: now,
            name: "".into(),
            description: "".into(),
            can_have_gateways: false,
            max_device_count: 0,
            max_gateway_count: 0,
            private_gateways_up: false,
            private_gateways_down: false,
            tags: fields::KeyValue::new(HashMap::new()),
        }
    }
}

#[derive(Queryable, Insertable, AsChangeset, PartialEq, Eq, Debug)]
#[diesel(table_name = tenant_user)]
pub struct TenantUser {
    pub tenant_id: fields::Uuid,
    pub user_id: fields::Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_admin: bool,
    pub is_device_admin: bool,
    pub is_gateway_admin: bool,
}

impl Default for TenantUser {
    fn default() -> Self {
        let now = Utc::now();

        TenantUser {
            tenant_id: Uuid::nil().into(),
            user_id: Uuid::nil().into(),
            created_at: now,
            updated_at: now,
            is_admin: false,
            is_device_admin: false,
            is_gateway_admin: false,
        }
    }
}

#[derive(Queryable, PartialEq, Eq, Debug)]
pub struct TenantUserListItem {
    pub tenant_id: fields::Uuid,
    pub user_id: fields::Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub email: String,
    pub is_admin: bool,
    pub is_device_admin: bool,
    pub is_gateway_admin: bool,
}

#[derive(Default, Clone)]
pub struct Filters {
    pub user_id: Option<Uuid>,
    pub search: Option<String>,
}

pub async fn create(t: Tenant) -> Result<Tenant, Error> {
    t.validate()?;

    let t: Tenant = diesel::insert_into(tenant::table)
        .values(&t)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, t.id.to_string()))?;
    info!(id = %t.id, "Tenant created");
    Ok(t)
}

pub async fn get(id: &Uuid) -> Result<Tenant, Error> {
    let t = tenant::dsl::tenant
        .find(&fields::Uuid::from(id))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;
    Ok(t)
}

pub async fn update(t: Tenant) -> Result<Tenant, Error> {
    t.validate()?;

    let t: Tenant = diesel::update(tenant::dsl::tenant.find(&t.id))
        .set((
            tenant::updated_at.eq(Utc::now()),
            tenant::name.eq(&t.name),
            tenant::description.eq(&t.description),
            tenant::can_have_gateways.eq(&t.can_have_gateways),
            tenant::max_device_count.eq(&t.max_device_count),
            tenant::max_gateway_count.eq(&t.max_gateway_count),
            tenant::private_gateways_up.eq(&t.private_gateways_up),
            tenant::private_gateways_down.eq(&t.private_gateways_down),
            tenant::tags.eq(&t.tags),
        ))
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, t.id.to_string()))?;
    info!(id = %t.id, "Tenant updated");
    Ok(t)
}

pub async fn delete(id: &Uuid) -> Result<(), Error> {
    let ra = diesel::delete(tenant::dsl::tenant.find(&fields::Uuid::from(id)))
        .execute(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;

    if ra == 0 {
        return Err(Error::NotFound(id.to_string()));
    }
    info!(id = %id, "Tenant deleted");
    Ok(())
}

pub async fn get_count(filters: &Filters) -> Result<i64, Error> {
    let mut q = tenant::dsl::tenant
        .left_join(tenant_user::table)
        .into_boxed();

    if let Some(user_id) = &filters.user_id {
        q = q.filter(tenant_user::dsl::user_id.eq(fields::Uuid::from(user_id)));
    }

    if let Some(search) = &filters.search {
        #[cfg(feature = "postgres")]
        {
            q = q.filter(tenant::dsl::name.ilike(format!("%{}%", search)));
        }
        #[cfg(feature = "sqlite")]
        {
            q = q.filter(tenant::dsl::name.like(format!("%{}%", search)));
        }
    }

    Ok(
        q.select(dsl::sql::<diesel::sql_types::BigInt>("count(distinct id)"))
            .first(&mut get_async_db_conn().await?)
            .await?,
    )
}

pub async fn list(limit: i64, offset: i64, filters: &Filters) -> Result<Vec<Tenant>, Error> {
    let mut q = tenant::dsl::tenant
        .left_join(tenant_user::table)
        .select(tenant::all_columns)
        .group_by(tenant::dsl::id)
        .order_by(tenant::dsl::name)
        .limit(limit)
        .offset(offset)
        .into_boxed();

    if let Some(user_id) = &filters.user_id {
        q = q.filter(tenant_user::dsl::user_id.eq(fields::Uuid::from(user_id)));
    }

    if let Some(search) = &filters.search {
        #[cfg(feature = "postgres")]
        {
            q = q.filter(tenant::dsl::name.ilike(format!("%{}%", search)));
        }
        #[cfg(feature = "sqlite")]
        {
            q = q.filter(tenant::dsl::name.like(format!("%{}%", search)));
        }
    }

    let items = q.load(&mut get_async_db_conn().await?).await?;
    Ok(items)
}

pub async fn add_user(tu: TenantUser) -> Result<TenantUser, Error> {
    let tu: TenantUser = diesel::insert_into(tenant_user::table)
        .values(&tu)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, tu.user_id.to_string()))?;
    info!(
        tenant_id = %tu.tenant_id,
        user_id = %tu.user_id,
        "Tenant user added"
    );
    Ok(tu)
}

pub async fn update_user(tu: TenantUser) -> Result<TenantUser, Error> {
    let tu: TenantUser = diesel::update(
        tenant_user::dsl::tenant_user
            .filter(tenant_user::dsl::tenant_id.eq(&tu.tenant_id))
            .filter(tenant_user::dsl::user_id.eq(&tu.user_id)),
    )
    .set(&tu)
    .get_result(&mut get_async_db_conn().await?)
    .await
    .map_err(|e| Error::from_diesel(e, tu.user_id.to_string()))?;
    info!(
        tenant_id = %tu.tenant_id,
        user_id = %tu.user_id,
        "Tenant user updated"
    );
    Ok(tu)
}

pub async fn get_user(tenant_id: &Uuid, user_id: &Uuid) -> Result<TenantUser, Error> {
    let tu: TenantUser = tenant_user::dsl::tenant_user
        .filter(tenant_user::dsl::tenant_id.eq(&fields::Uuid::from(tenant_id)))
        .filter(tenant_user::dsl::user_id.eq(&fields::Uuid::from(user_id)))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, user_id.to_string()))?;
    Ok(tu)
}

pub async fn get_user_count(tenant_id: &Uuid) -> Result<i64, Error> {
    let count = tenant_user::dsl::tenant_user
        .select(dsl::count_star())
        .filter(tenant_user::dsl::tenant_id.eq(fields::Uuid::from(tenant_id)))
        .first(&mut get_async_db_conn().await?)
        .await?;
    Ok(count)
}

pub async fn get_users(
    tenant_id: &Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<TenantUserListItem>, Error> {
    let items = tenant_user::dsl::tenant_user
        .inner_join(user::table)
        .select((
            tenant_user::dsl::tenant_id,
            tenant_user::dsl::user_id,
            tenant_user::dsl::created_at,
            tenant_user::dsl::updated_at,
            user::dsl::email,
            tenant_user::dsl::is_admin,
            tenant_user::dsl::is_device_admin,
            tenant_user::dsl::is_gateway_admin,
        ))
        .filter(tenant_user::dsl::tenant_id.eq(&fields::Uuid::from(tenant_id)))
        .order_by(user::dsl::email)
        .limit(limit)
        .offset(offset)
        .load(&mut get_async_db_conn().await?)
        .await?;

    Ok(items)
}

pub async fn delete_user(tenant_id: &Uuid, user_id: &Uuid) -> Result<(), Error> {
    let ra = diesel::delete(
        tenant_user::dsl::tenant_user
            .filter(tenant_user::dsl::tenant_id.eq(&fields::Uuid::from(tenant_id)))
            .filter(tenant_user::dsl::user_id.eq(&fields::Uuid::from(user_id))),
    )
    .execute(&mut get_async_db_conn().await?)
    .await?;
    if ra == 0 {
        return Err(Error::NotFound(user_id.to_string()));
    }
    info!(
        tenant_id = %tenant_id,
        user_id = %user_id,
        "Tenant user deleted"
    );
    Ok(())
}

pub async fn get_tenant_users_for_user(user_id: &Uuid) -> Result<Vec<TenantUser>, Error> {
    let items = tenant_user::dsl::tenant_user
        .filter(tenant_user::dsl::user_id.eq(&fields::Uuid::from(user_id)))
        .load(&mut get_async_db_conn().await?)
        .await?;
    Ok(items)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::storage::user::test::create_user;
    use crate::test;
    use chrono::SubsecRound;
    use std::str::FromStr;
    use uuid::Uuid;

    struct FilterTest<'a> {
        filter: Filters,
        ts: Vec<&'a Tenant>,
        count: usize,
        limit: i64,
        offset: i64,
    }

    pub async fn create_tenant() -> Tenant {
        let t = Tenant {
            id: Uuid::new_v4().into(),
            created_at: Utc::now().round_subsecs(1),
            updated_at: Utc::now().round_subsecs(1),
            name: "test t".into(),
            description: "test description".into(),
            can_have_gateways: true,
            max_device_count: 20,
            max_gateway_count: 10,
            private_gateways_up: true,
            private_gateways_down: true,
            tags: fields::KeyValue::new(HashMap::new()),
        };
        create(t).await.unwrap()
    }

    #[tokio::test]
    async fn test_tenant() {
        let _guard = test::prepare().await;

        // delete default tenant
        delete(&Uuid::from_str("52f14cd4-c6f1-4fbd-8f87-4025e1d49242").unwrap())
            .await
            .unwrap();

        let mut t = create_tenant().await;

        // get
        let t_get = get(&t.id).await.unwrap();
        assert_eq!(t, t_get);

        // update
        t.name = "new t".into();
        t = update(t).await.unwrap();
        let t_get = get(&t.id).await.unwrap();
        assert_eq!(t, t_get);

        // add tenant user for filter by user_id test
        let user = create_user().await;

        let tu = TenantUser {
            tenant_id: t.id,
            user_id: user.id.into(),
            is_admin: true,
            ..Default::default()
        };

        add_user(tu).await.unwrap();

        // get_count and list
        let tests = vec![
            FilterTest {
                filter: Filters {
                    search: None,
                    user_id: None,
                },
                ts: vec![&t],
                count: 1,
                limit: 10,
                offset: 0,
            },
            FilterTest {
                filter: Filters {
                    search: Some("bt".into()),
                    user_id: None,
                },
                ts: vec![],
                count: 0,
                limit: 10,
                offset: 0,
            },
            FilterTest {
                filter: Filters {
                    search: Some("t".into()),
                    user_id: None,
                },
                ts: vec![&t],
                count: 1,
                limit: 10,
                offset: 0,
            },
            FilterTest {
                filter: Filters {
                    search: Some("t".into()),
                    user_id: None,
                },
                ts: vec![],
                count: 1,
                limit: 0,
                offset: 0,
            },
            FilterTest {
                filter: Filters {
                    search: Some("t".into()),
                    user_id: None,
                },
                ts: vec![],
                count: 1,
                limit: 10,
                offset: 10,
            },
            FilterTest {
                filter: Filters {
                    user_id: Some(user.id.into()),
                    search: None,
                },
                ts: vec![&t],
                count: 1,
                limit: 10,
                offset: 0,
            },
        ];
        for tst in tests {
            let count = get_count(&tst.filter).await.unwrap() as usize;
            assert_eq!(tst.count, count);

            let items = list(tst.limit, tst.offset, &tst.filter).await.unwrap();
            assert_eq!(
                tst.ts
                    .iter()
                    .map(|t| { t.id.to_string() })
                    .collect::<String>(),
                items
                    .iter()
                    .map(|t| { t.id.to_string() })
                    .collect::<String>()
            );
        }

        // delete
        delete(&t.id).await.unwrap();
        assert!(delete(&t.id).await.is_err());
    }

    #[tokio::test]
    async fn test_tenant_user() {
        let _guard = test::prepare().await;

        let t = create_tenant().await;
        let user = create_user().await;

        let tu = TenantUser {
            tenant_id: t.id,
            user_id: user.id.into(),
            is_admin: true,
            ..Default::default()
        };

        // add user
        let tu = add_user(tu).await.unwrap();

        // get
        let tu_get = get_user(&t.id, &user.id).await.unwrap();
        assert_eq!(tu, tu_get);

        // get count and list
        let count = get_user_count(&t.id).await.unwrap();
        assert_eq!(1, count);

        // get users
        let users = get_users(&t.id, 10, 0).await.unwrap();
        assert_eq!(user.id, users[0].user_id);

        // delete
        delete_user(&t.id, &user.id).await.unwrap();
    }
}

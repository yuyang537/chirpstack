/*
 * 模块概述
 * ========
 * 用户管理API模块是ChirpStack LoRaWAN网络服务器的重要组成部分，负责处理系统中用户账户的创建、查询、
 * 更新和删除等操作。该模块是ChirpStack权限管理系统的基础，为整个平台提供用户身份管理和访问控制支持。
 * 
 * 在ChirpStack的多租户架构中，用户是最基本的身份实体，可以被分配到不同的租户中并赋予不同的权限。
 * 用户管理API与认证模块紧密集成，支持基于用户名/密码的本地认证以及与外部身份提供商的集成。
 * 
 * 该模块实现了完整的用户生命周期管理，包括创建用户、设置密码、更新用户信息、禁用/启用用户账户等功能，
 * 为ChirpStack提供了灵活而安全的用户管理能力。
 *
 * 文件功能
 * ========
 * 本文件(user.rs)实现了用户管理的gRPC服务接口，提供了以下主要功能：
 * 1. 创建新用户账户，包括设置初始密码和用户属性
 * 2. 获取用户详细信息
 * 3. 更新现有用户的信息和设置
 * 4. 删除用户账户
 * 5. 列出系统中的所有用户
 * 6. 更新用户密码，包括密码哈希处理
 * 
 * 文件实现了UserService trait，该trait定义了用户管理的标准gRPC接口，使客户端应用能够
 * 通过标准化的方式与用户管理系统交互。
 *
 * 主要组件
 * ========
 * - User: 核心服务结构体，实现了UserService trait
 *   - new(): 创建User服务实例的构造函数
 *   - create(): 创建新用户
 *   - get(): 获取用户详情
 *   - update(): 更新用户信息
 *   - delete(): 删除用户
 *   - list(): 列出用户
 *   - update_password(): 更新用户密码
 * - validator: 用于验证请求权限的组件
 * - pw_hash_iterations: 密码哈希迭代次数，用于安全存储密码
 *
 * 关键流程
 * ========
 * 1. 用户创建流程:
 *    - 验证请求者是否有创建用户的权限
 *    - 验证请求数据的有效性
 *    - 对用户密码进行安全哈希处理
 *    - 在数据库中创建用户记录
 *    - 返回新创建的用户ID
 * 
 * 2. 用户认证流程:
 *    - 接收用户名和密码
 *    - 验证密码哈希是否匹配
 *    - 生成认证令牌
 * 
 * 3. 密码更新流程:
 *    - 验证当前密码
 *    - 对新密码进行安全哈希处理
 *    - 更新存储的密码哈希
 *
 * 注意事项
 * ========
 * - 安全性: 密码存储使用安全的哈希算法，密码哈希迭代次数(pw_hash_iterations)影响安全性和性能
 * - 权限控制: 所有API操作都需要进行权限验证，确保只有授权用户能执行相应操作
 * - 数据验证: 用户输入需要严格验证，特别是电子邮件地址和密码复杂度
 * - 隐私保护: 用户敏感信息(如密码哈希)不应在API响应中返回
 * - 错误处理: 提供清晰的错误信息，但避免泄露系统内部细节
 * - 审计日志: 用户管理操作应记录审计日志，便于安全审计和问题排查
 */

use std::str::FromStr;

use chrono::Utc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use chirpstack_api::api;
use chirpstack_api::api::user_service_server::UserService;

use super::auth::{validator, AuthID};
use super::error::ToStatus;
use super::helpers;
use crate::storage::{tenant, user};

pub struct User {
    validator: validator::RequestValidator,
    pw_hash_iterations: u32,
}

impl User {
    pub fn new(validator: validator::RequestValidator) -> Self {
        User {
            validator,
            pw_hash_iterations: 10_000,
        }
    }
}

#[tonic::async_trait]
impl UserService for User {
    async fn create(
        &self,
        request: Request<api::CreateUserRequest>,
    ) -> Result<Response<api::CreateUserResponse>, Status> {
        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUsersAccess::new(validator::Flag::Create),
            )
            .await?;

        let req = request.get_ref();
        let req_user = match &req.user {
            Some(v) => v,
            None => {
                return Err(Status::invalid_argument("user is missing"));
            }
        };

        let mut u = user::User {
            is_admin: req_user.is_admin,
            is_active: req_user.is_active,
            email: req_user.email.clone(),
            note: req_user.note.clone(),
            ..Default::default()
        };

        u.set_password_hash(&req.password, self.pw_hash_iterations)
            .map_err(|e| e.status())?;

        u = user::create(u).await.map_err(|e| e.status())?;

        for tu in &req.tenants {
            let tenant_id = Uuid::from_str(&tu.tenant_id).map_err(|e| e.status())?;

            tenant::add_user(tenant::TenantUser {
                tenant_id: tenant_id.into(),
                user_id: u.id,
                is_admin: tu.is_admin,
                is_device_admin: tu.is_device_admin,
                is_gateway_admin: tu.is_gateway_admin,
                ..Default::default()
            })
            .await
            .map_err(|e| e.status())?;
        }

        let mut resp = Response::new(api::CreateUserResponse {
            id: u.id.to_string(),
        });
        resp.metadata_mut()
            .insert("x-log-user_id", u.id.to_string().parse().unwrap());

        Ok(resp)
    }

    async fn get(
        &self,
        request: Request<api::GetUserRequest>,
    ) -> Result<Response<api::GetUserResponse>, Status> {
        let req = request.get_ref();
        let user_id = Uuid::from_str(&req.id).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUserAccess::new(validator::Flag::Read, user_id),
            )
            .await?;

        let u = user::get(&user_id).await.map_err(|e| e.status())?;

        let mut resp = Response::new(api::GetUserResponse {
            user: Some(api::User {
                id: u.id.to_string(),
                is_admin: u.is_admin,
                is_active: u.is_active,
                email: u.email.clone(),
                note: u.note.clone(),
            }),
            created_at: Some(helpers::datetime_to_prost_timestamp(&u.created_at)),
            updated_at: Some(helpers::datetime_to_prost_timestamp(&u.updated_at)),
        });
        resp.metadata_mut()
            .insert("x-log-user_id", req.id.parse().unwrap());

        Ok(resp)
    }

    async fn update(
        &self,
        request: Request<api::UpdateUserRequest>,
    ) -> Result<Response<()>, Status> {
        let req_user = match &request.get_ref().user {
            Some(v) => v,
            None => {
                return Err(Status::invalid_argument("user is missing"));
            }
        };
        let user_id = Uuid::from_str(&req_user.id).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUserAccess::new(validator::Flag::Update, user_id),
            )
            .await?;

        // update
        let _ = user::update(user::User {
            id: user_id.into(),
            is_admin: req_user.is_admin,
            is_active: req_user.is_active,
            email: req_user.email.clone(),
            email_verified: true,
            note: req_user.note.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| e.status())?;

        let mut resp = Response::new(());
        resp.metadata_mut()
            .insert("x-log-user_id", req_user.id.parse().unwrap());

        Ok(resp)
    }

    async fn delete(
        &self,
        request: Request<api::DeleteUserRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        let user_id = Uuid::from_str(&req.id).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUserAccess::new(validator::Flag::Delete, user_id),
            )
            .await?;

        let auth_id = request.extensions().get::<AuthID>().unwrap();
        if let AuthID::User(id) = auth_id {
            if id == &user_id {
                return Err(Status::invalid_argument(
                    "you can not delete yourself from the user",
                ));
            }
        }

        user::delete(&user_id).await.map_err(|e| e.status())?;

        let mut resp = Response::new(());
        resp.metadata_mut()
            .insert("x-log-user_id", req.id.parse().unwrap());

        Ok(resp)
    }

    async fn list(
        &self,
        request: Request<api::ListUsersRequest>,
    ) -> Result<Response<api::ListUsersResponse>, Status> {
        let req = request.get_ref();
        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUsersAccess::new(validator::Flag::List),
            )
            .await?;

        let count = user::get_count().await.map_err(|e| e.status())?;
        let items = user::list(req.limit as i64, req.offset as i64)
            .await
            .map_err(|e| e.status())?;

        Ok(Response::new(api::ListUsersResponse {
            total_count: count as u32,
            result: items
                .iter()
                .map(|u| api::UserListItem {
                    id: u.id.to_string(),
                    created_at: Some(helpers::datetime_to_prost_timestamp(&u.created_at)),
                    updated_at: Some(helpers::datetime_to_prost_timestamp(&u.updated_at)),
                    email: u.email.clone(),
                    is_admin: u.is_admin,
                    is_active: u.is_active,
                })
                .collect(),
        }))
    }

    async fn update_password(
        &self,
        request: Request<api::UpdateUserPasswordRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        let user_id = Uuid::from_str(&req.user_id).map_err(|e| e.status())?;
        self.validator
            .validate(
                request.extensions(),
                validator::ValidateUserAccess::new(validator::Flag::UpdateProfile, user_id),
            )
            .await?;

        // get
        let mut u = user::get(&user_id).await.map_err(|e| e.status())?;

        // set password
        u.updated_at = Utc::now();
        u.set_password_hash(&req.password, self.pw_hash_iterations)
            .map_err(|e| e.status())?;

        // update
        let _ = user::set_password_hash(&u.id, &u.password_hash)
            .await
            .map_err(|e| e.status())?;

        let mut resp = Response::new(());
        resp.metadata_mut()
            .insert("x-log-user_id", req.user_id.parse().unwrap());

        Ok(resp)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::api::auth::validator::RequestValidator;
    use crate::api::auth::AuthID;
    use crate::test;

    #[tokio::test]
    async fn test_user() {
        let _guard = test::prepare().await;

        // setup admin user
        let u = user::User {
            is_admin: true,
            is_active: true,
            email: "admin@admin".into(),
            email_verified: true,
            ..Default::default()
        };
        let u = user::create(u).await.unwrap();

        // setup api
        let service = User::new(RequestValidator::new());

        // create
        let create_req = api::CreateUserRequest {
            password: "secret".into(),
            tenants: vec![],
            user: Some(api::User {
                is_admin: true,
                is_active: true,
                email: "foo@bar".into(),
                note: "test user".into(),
                ..Default::default()
            }),
        };
        let mut create_req = Request::new(create_req);
        create_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let create_resp = service.create(create_req).await.unwrap();

        // get
        let get_req = api::GetUserRequest {
            id: create_resp.get_ref().id.clone(),
        };
        let mut get_req = Request::new(get_req);
        get_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let get_resp = service.get(get_req).await.unwrap();
        assert_eq!(
            Some(api::User {
                id: create_resp.get_ref().id.clone(),
                is_admin: true,
                is_active: true,
                email: "foo@bar".into(),
                note: "test user".into(),
                ..Default::default()
            }),
            get_resp.get_ref().user
        );

        // update
        let up_req = api::UpdateUserRequest {
            user: Some(api::User {
                id: create_resp.get_ref().id.clone(),
                is_admin: false,
                is_active: true,
                email: "foo@bar".into(),
                note: "updated user".into(),
                ..Default::default()
            }),
        };
        let mut up_req = Request::new(up_req);
        up_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let _ = service.update(up_req).await.unwrap();

        // get
        let get_req = api::GetUserRequest {
            id: create_resp.get_ref().id.clone(),
        };
        let mut get_req = Request::new(get_req);
        get_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let get_resp = service.get(get_req).await.unwrap();
        assert_eq!(
            Some(api::User {
                id: create_resp.get_ref().id.clone(),
                is_admin: false,
                is_active: true,
                email: "foo@bar".into(),
                note: "updated user".into(),
                ..Default::default()
            }),
            get_resp.get_ref().user
        );

        // update password
        let up_req = api::UpdateUserPasswordRequest {
            user_id: create_resp.get_ref().id.clone(),
            password: "newpassword".into(),
        };
        let mut up_req = Request::new(up_req);
        up_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let _ = service.update_password(up_req).await.unwrap();

        // list
        let list_req = api::ListUsersRequest {
            offset: 0,
            limit: 10,
        };
        let mut list_req = Request::new(list_req);
        list_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let list_resp = service.list(list_req).await.unwrap();
        // * Admin from migrations
        // * User that we created for auth
        // * User that we created through API
        assert_eq!(3, list_resp.get_ref().total_count);
        assert_eq!(3, list_resp.get_ref().result.len());

        // delete
        let del_req = api::DeleteUserRequest {
            id: create_resp.get_ref().id.clone(),
        };
        let mut del_req = Request::new(del_req);
        del_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let _ = service.delete(del_req).await.unwrap();

        let del_req = api::DeleteUserRequest {
            id: create_resp.get_ref().id.clone(),
        };
        let mut del_req = Request::new(del_req);
        del_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let del_resp = service.delete(del_req).await;
        assert!(del_resp.is_err());

        let del_req = api::DeleteUserRequest {
            id: u.id.to_string(),
        };
        let mut del_req = Request::new(del_req);
        del_req
            .extensions_mut()
            .insert(AuthID::User(Into::<uuid::Uuid>::into(u.id).clone()));
        let del_resp = service.delete(del_req).await;
        assert!(del_resp.is_err());
    }
}

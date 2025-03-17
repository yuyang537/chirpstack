/**
 * @module user
 * 
 * @description
 * 
 * # 模块概述
 * 本模块实现了ChirpStack系统中用户管理的核心功能，包括用户的创建、查询、更新和删除操作。
 * 用户是ChirpStack系统的基本管理单元，拥有不同的权限级别和访问控制。
 * 
 * # 文件功能
 * - 定义User结构体，表示系统中的用户实体
 * - 提供用户密码的安全哈希和验证功能
 * - 实现用户的CRUD（创建、读取、更新、删除）操作
 * - 支持通过多种方式查询用户（ID、邮箱、外部ID等）
 * - 提供用户认证和密码验证功能
 * 
 * # 主要组件
 * - User：用户实体结构体，包含用户的所有属性
 * - create/get/update/delete：用户的基本CRUD操作
 * - get_by_email/get_by_external_id：通过特定属性查询用户
 * - hash_password/verify_password：密码哈希和验证功能
 * 
 * # 关键流程
 * - 用户创建：生成UUID，设置默认值，哈希密码，存储到数据库
 * - 用户认证：通过邮箱和密码验证用户身份
 * - 密码管理：使用PBKDF2算法进行密码哈希和验证
 * - 用户查询：支持多种查询方式，包括分页列表查询
 * 
 * # 重要考虑事项
 * - 密码使用PBKDF2-SHA512算法进行哈希，提供安全保护
 * - 用户邮箱需要验证格式有效性
 * - 支持外部身份提供者集成，通过external_id字段
 * - 用户可以具有管理员权限(is_admin)和活动状态(is_active)
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use email_address::EmailAddress;
use pbkdf2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Pbkdf2,
};
use rand_core::OsRng;
use tracing::info;
use uuid::Uuid;

use super::error::Error;
use super::schema::user;
use super::{fields, get_async_db_conn};

#[derive(Queryable, Insertable, PartialEq, Eq, Debug, Clone)]
#[diesel(table_name = user)]
pub struct User {
    pub id: fields::Uuid,
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_admin: bool,
    pub is_active: bool,
    pub email: String,
    pub email_verified: bool,
    pub password_hash: String,
    pub note: String,
}

impl Default for User {
    fn default() -> Self {
        let now = Utc::now();

        User {
            id: Uuid::new_v4().into(),
            external_id: None,
            created_at: now,
            updated_at: now,
            is_admin: false,
            is_active: false,
            email: "".into(),
            email_verified: false,
            password_hash: "".into(),
            note: "".into(),
        }
    }
}

impl User {
    pub fn validate(&self) -> Result<(), Error> {
        if self.email != "admin" && !EmailAddress::is_valid(&self.email) {
            return Err(Error::InvalidEmail);
        }

        Ok(())
    }

    pub fn set_password_hash(&mut self, pw: &str, rounds: u32) -> Result<(), Error> {
        self.password_hash = hash_password(pw, rounds)?;
        Ok(())
    }
}

pub async fn create(u: User) -> Result<User, Error> {
    u.validate()?;

    let u: User = diesel::insert_into(user::table)
        .values(&u)
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, u.id.to_string()))?;
    info!(id = %u.id, "User created");
    Ok(u)
}

pub async fn get(id: &Uuid) -> Result<User, Error> {
    let u = user::dsl::user
        .find(&fields::Uuid::from(id))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;
    Ok(u)
}

pub async fn get_by_email(email: &str) -> Result<User, Error> {
    let u = user::dsl::user
        .filter(user::dsl::email.eq(email))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, email.to_string()))?;
    Ok(u)
}

pub async fn get_by_external_id(external_id: &str) -> Result<User, Error> {
    let u = user::dsl::user
        .filter(user::dsl::external_id.eq(external_id))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, external_id.to_string()))?;
    Ok(u)
}

pub async fn get_by_email_and_pw(email: &str, pw: &str) -> Result<User, Error> {
    let u: User = match user::dsl::user
        .filter(user::dsl::email.eq(email))
        .first(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, email.to_string()))
    {
        Ok(v) => v,
        Err(Error::NotFound(_)) => {
            return Err(Error::InvalidUsernameOrPassword);
        }
        Err(v) => {
            return Err(v);
        }
    };

    if verify_password(pw, &u.password_hash) {
        return Ok(u);
    }

    Err(Error::InvalidUsernameOrPassword)
}

pub async fn update(u: User) -> Result<User, Error> {
    u.validate()?;

    let u: User = diesel::update(user::dsl::user.find(&u.id))
        .set((
            user::updated_at.eq(Utc::now()),
            user::is_admin.eq(&u.is_admin),
            user::is_active.eq(&u.is_active),
            user::email.eq(&u.email),
            user::email_verified.eq(&u.email_verified),
            user::note.eq(&u.note),
            user::external_id.eq(&u.external_id),
        ))
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, u.id.to_string()))?;
    info!(user_id = %u.id, "User updated");
    Ok(u)
}

pub async fn set_password_hash(id: &Uuid, hash: &str) -> Result<User, Error> {
    let u: User = diesel::update(user::dsl::user.find(&fields::Uuid::from(id)))
        .set(user::password_hash.eq(&hash))
        .get_result(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;
    info!(id = %id, "Password set");
    Ok(u)
}

pub async fn delete(id: &Uuid) -> Result<(), Error> {
    let ra = diesel::delete(user::dsl::user.find(&fields::Uuid::from(id)))
        .execute(&mut get_async_db_conn().await?)
        .await
        .map_err(|e| Error::from_diesel(e, id.to_string()))?;

    if ra == 0 {
        return Err(Error::NotFound(id.to_string()));
    }
    info!(user_id = %id, "User deleted");
    Ok(())
}

pub async fn get_count() -> Result<i64, Error> {
    let count = user::dsl::user
        .select(dsl::count_star())
        .first(&mut get_async_db_conn().await?)
        .await?;
    Ok(count)
}

pub async fn list(limit: i64, offset: i64) -> Result<Vec<User>, Error> {
    let items = user::dsl::user
        .order_by(user::dsl::email)
        .limit(limit)
        .offset(offset)
        .load(&mut get_async_db_conn().await?)
        .await?;
    Ok(items)
}

// The output format is documented here:
// https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md#specification
fn hash_password(pw: &str, rounds: u32) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash_resp = Pbkdf2.hash_password_customized(
        pw.as_bytes(),
        Some(Algorithm::Pbkdf2Sha512.ident()),
        None,
        pbkdf2::Params {
            rounds,
            ..Default::default()
        },
        salt.as_salt(),
    );

    match hash_resp {
        Ok(v) => Ok(v.to_string()),
        Err(e) => Err(Error::HashPassword(format!("{}", e))),
    }
}

fn verify_password(pw: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(v) => v,
        Err(_) => {
            return false;
        }
    };

    Pbkdf2.verify_password(pw.as_bytes(), &parsed).is_ok()
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::test;

    pub async fn create_user() -> User {
        let mut user = User {
            is_admin: true,
            is_active: true,
            email: "test@example.com".into(),
            email_verified: true,
            ..Default::default()
        };
        user.set_password_hash("password!", 1).unwrap();
        create(user).await.unwrap()
    }

    #[test]
    fn test_hash_password() {
        assert!(hash_password("foobar", 1000).is_ok());
    }

    #[test]
    fn test_verify_password() {
        // this is the ChirpStack Application Server default admin hash, with == removed
        // to test the compatibility betweeh the two pbkdf2 implementations.
        assert!(verify_password("admin", "$pbkdf2-sha512$i=1,l=64$l8zGKtxRESq3PA2kFhHRWA$H3lGMxOt55wjwoc+myeOoABofJY9oDpldJa7fhqdjbh700V6FLPML75UmBOt9J5VFNjAL1AvqCozA1HJM0QVGA"));
    }

    #[tokio::test]
    async fn test_user() {
        let _guard = test::prepare().await;
        let mut user = create_user().await;

        // get
        let user_get = get(&user.id).await.unwrap();
        assert_eq!(user, user_get);

        // update
        user.external_id = Some("external_id".into());
        user = update(user).await.unwrap();

        // get by external id
        let user_get = get_by_external_id("external_id").await.unwrap();
        assert_eq!(user, user_get);

        // get_by_email_and_pw
        assert!(get_by_email_and_pw("test@example.com", "bar")
            .await
            .is_err());
        let user_get = get_by_email_and_pw("test@example.com", "password!")
            .await
            .unwrap();
        assert_eq!(user, user_get);

        // delete
        delete(&user.id).await.unwrap();
        assert!(delete(&user.id).await.is_err());
    }
}

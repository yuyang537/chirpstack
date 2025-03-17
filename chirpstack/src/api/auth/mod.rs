/*
 * 模块概述
 * ========
 * 认证(Auth)模块是ChirpStack LoRaWAN网络服务器的核心安全组件，负责实现API访问的身份验证和授权机制。
 * 该模块确保只有经过身份验证的用户和应用程序才能访问ChirpStack的API接口，并根据其权限级别限制对特定资源的访问。
 * 本模块实现了基于JWT(JSON Web Token)的认证系统，支持用户账户和API密钥两种认证方式，为ChirpStack提供了灵活而安全的访问控制机制。
 * 
 * 认证模块与ChirpStack的其他组件紧密集成，为整个系统提供统一的安全层，保护敏感的网络配置和设备数据。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是认证模块的入口点，定义了认证系统的核心数据结构和拦截器功能。
 * 它实现了一个Tonic中间件拦截器，用于处理所有进入ChirpStack API的请求，验证认证令牌并提取身份信息。
 * 文件定义了AuthID枚举，用于表示不同类型的认证身份（无认证、用户认证和API密钥认证）。
 *
 * 主要组件
 * ========
 * - AuthID: 表示认证身份类型的枚举（None、User、Key）
 * - auth_interceptor: Tonic中间件拦截器，处理所有API请求的认证逻辑
 * - claims: 子模块，处理JWT令牌的生成和验证
 * - error: 子模块，定义认证相关的错误类型
 * - validator: 子模块，实现资源访问权限的验证逻辑
 *
 * 关键流程
 * ========
 * 1. 客户端在请求头中提供Bearer令牌
 * 2. auth_interceptor拦截请求并提取Authorization头
 * 3. 使用JWT库验证令牌的有效性和签名
 * 4. 从令牌中提取用户ID或API密钥ID
 * 5. 根据令牌类型(typ)确定认证身份类型
 * 6. 将认证身份信息存储在请求的扩展中，供后续处理使用
 *
 * 注意事项
 * ========
 * - 认证令牌必须使用正确的格式："Bearer <TOKEN>"
 * - 用户令牌有过期时间限制，而API密钥令牌通常不设置过期时间
 * - 令牌验证失败会返回unauthenticated状态错误
 * - 某些API方法可能不要求认证，这种情况下会设置AuthID::None
 * - 令牌的安全性依赖于配置中设置的API密钥(api.secret)
 * - 该模块仅处理身份验证，具体的授权逻辑由validator子模块实现
 */

use crate::config;
use tonic::{Request, Status};
use uuid::Uuid;

pub mod claims;
pub mod error;
pub mod validator;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum AuthID {
    None,
    User(Uuid),
    Key(Uuid),
}

pub fn auth_interceptor(mut req: Request<()>) -> Result<Request<()>, Status> {
    let conf = config::get();

    let auth_str = match req.metadata().get("authorization") {
        Some(v) => match v.to_str() {
            Ok(vv) => vv,
            Err(e) => {
                return Err(Status::unauthenticated(format!("{}", e)));
            }
        },
        _ => {
            // some API methods do not require the authorization metadata. When it is not available
            // we do not error. Each will perform its own authorization.
            req.extensions_mut().insert(AuthID::None);
            return Ok(req);
        }
    };

    let auth_str = match auth_str.strip_prefix("Bearer ") {
        Some(v) => v,
        None => {
            return Err(Status::unauthenticated(
                "authorization metadata must be in format 'Bearer <TOKEN>",
            ));
        }
    };

    let token = match claims::AuthClaim::decode(auth_str, conf.api.secret.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            return Err(Status::unauthenticated(format!("{}", e)));
        }
    };

    let id = match Uuid::parse_str(&token.sub) {
        Ok(v) => v,
        Err(e) => {
            return Err(Status::unauthenticated(format!("{}", e)));
        }
    };

    match token.typ.as_ref() {
        "user" => {
            req.extensions_mut().insert(AuthID::User(id));
        }
        "key" => {
            req.extensions_mut().insert(AuthID::Key(id));
        }
        _ => {
            return Err(Status::unauthenticated(format!(
                "invalid token typ: {}",
                token.typ
            )));
        }
    };

    Ok(req)
}

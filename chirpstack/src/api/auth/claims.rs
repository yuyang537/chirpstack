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
 * 本文件(claims.rs)实现了JWT令牌的生成、编码和解码功能。它定义了AuthClaim结构体，表示JWT令牌中包含的声明信息。
 * 文件提供了创建用户令牌和API密钥令牌的方法，以及验证令牌有效性的功能。
 * 这些功能是ChirpStack认证系统的核心，确保了API访问的安全性和身份验证的可靠性。
 *
 * 主要组件
 * ========
 * - AuthClaim: JWT令牌声明结构体，包含受众(aud)、过期时间(exp)、发行者(iss)、主题(sub)和类型(typ)等字段
 * - new_for_user(): 创建用户认证令牌，包含过期时间（通常为24小时）
 * - new_for_api_key(): 创建API密钥认证令牌，通常不设置过期时间
 * - encode(): 将AuthClaim结构编码为JWT令牌字符串
 * - decode(): 验证JWT令牌并解码为AuthClaim结构
 * - is_default(): 辅助函数，用于序列化时跳过默认值字段
 *
 * 关键流程
 * ========
 * 1. 用户登录或API密钥创建时，生成相应的AuthClaim结构
 * 2. 使用encode()方法将AuthClaim编码为JWT令牌字符串
 * 3. 客户端在API请求中提供JWT令牌
 * 4. 服务器使用decode()方法验证令牌并提取身份信息
 * 5. 对于用户令牌，还会检查过期时间是否有效
 *
 * 注意事项
 * ========
 * - 用户令牌设置了24小时的过期时间，而API密钥令牌通常不过期
 * - JWT令牌的安全性依赖于配置中设置的密钥(api.secret)
 * - 令牌验证会检查受众(aud)字段是否为"chirpstack"
 * - 令牌类型(typ)字段用于区分用户令牌("user")和API密钥令牌("key")
 * - 主题(sub)字段存储用户ID或API密钥ID
 * - 令牌过期后需要重新登录获取新令牌
 * - 该实现使用HS256算法进行JWT签名
 */

use std::collections::HashSet;
use std::ops::Add;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct AuthClaim {
    pub aud: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub exp: Option<usize>,
    pub iss: String,
    pub sub: String,
    pub typ: String,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

impl AuthClaim {
    pub fn new_for_user(id: &Uuid) -> Self {
        let nbf: DateTime<Utc> = Utc::now();
        let exp = nbf.add(Duration::try_days(1).unwrap());

        AuthClaim {
            aud: "chirpstack".to_string(),
            exp: Some(exp.timestamp() as usize),
            iss: "chirpstack".to_string(),
            sub: id.to_string(),
            typ: "user".to_string(),
        }
    }

    pub fn new_for_api_key(id: &Uuid) -> Self {
        AuthClaim {
            aud: "chirpstack".to_string(),
            iss: "chirpstack".to_string(),
            sub: id.to_string(),
            typ: "key".to_string(),
            exp: None,
        }
    }

    pub fn encode(&self, secret: &[u8]) -> Result<String> {
        Ok(encode(
            &Header::default(),
            self,
            &EncodingKey::from_secret(secret),
        )?)
    }

    pub fn decode(token: &str, secret: &[u8]) -> Result<Self> {
        let mut val = Validation::new(Algorithm::HS256);
        val.set_audience(&["chirpstack"]);
        val.required_spec_claims = HashSet::new(); // make the 'exp' optional

        let claim = decode::<AuthClaim>(token, &DecodingKey::from_secret(secret), &val)?;
        Ok(claim.claims)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_for_user() {
        let secrect = &"verysecret";
        let other_secret = &"notsosecret";
        let user_id = Uuid::new_v4();
        let key_id = Uuid::new_v4();

        let nbf: DateTime<Utc> = Utc::now();
        let exp = nbf.add(-Duration::try_days(1).unwrap());

        let claim = AuthClaim::new_for_api_key(&key_id);
        assert_eq!("key", claim.typ);
        assert_eq!(key_id.to_string(), claim.sub);

        let token = claim.encode(secrect.as_ref()).unwrap();
        let decoded = AuthClaim::decode(&token, secrect.as_ref()).unwrap();
        assert_eq!(claim, decoded);

        // user token
        let mut claim = AuthClaim::new_for_user(&user_id);
        assert_eq!("user", claim.typ);
        assert_eq!(user_id.to_string(), claim.sub);

        let token = claim.encode(secrect.as_ref()).unwrap();
        let decoded = AuthClaim::decode(&token, secrect.as_ref()).unwrap();
        assert_eq!(claim, decoded);

        // different key
        assert!(AuthClaim::decode(&token, other_secret.as_ref()).is_err());

        // expired
        claim.exp = Some(exp.timestamp() as usize);
        let token = claim.encode(secrect.as_ref()).unwrap();
        assert!(AuthClaim::decode(&token, secrect.as_ref()).is_err());
    }
}

/*
 * 模块概述
 * ========
 * OAuth2模块是ChirpStack LoRaWAN网络服务器认证框架的重要组件，负责实现与外部OAuth2
 * 身份提供商的集成。该模块提供了完整的OAuth2授权流程支持，允许用户使用现有的外部
 * 账户（如Google、GitHub、Clerk等）登录ChirpStack系统。
 * 
 * OAuth2是一种行业标准的授权协议，它允许第三方应用程序在不直接处理用户凭据的情况下
 * 获取对用户资源的访问权限。通过实现OAuth2集成，ChirpStack能够提供更安全、更灵活的
 * 用户认证方式，同时简化用户管理流程。
 * 
 * 该模块作为ChirpStack认证系统的一部分，与内部认证和OpenID Connect认证一起，为系统
 * 提供了全面的身份验证解决方案。
 *
 * 文件功能
 * ========
 * 本文件(oauth2.rs)实现了OAuth2授权流程，提供了以下主要功能：
 * 1. OAuth2登录流程处理：初始化授权请求和处理回调
 * 2. 用户信息获取：从OAuth2提供商获取用户信息
 * 3. PKCE增强安全性：实现PKCE（Proof Key for Code Exchange）流程
 * 4. 特定提供商集成：支持Clerk等OAuth2提供商的特定实现
 *
 * 主要组件
 * ========
 * - login_handler(): 处理OAuth2登录请求，生成授权URL
 * - callback_handler(): 处理OAuth2提供商的回调
 * - get_client(): 创建OAuth2客户端
 * - get_user(): 使用授权码获取用户信息
 * - get_clerk_user(): 从Clerk获取用户信息
 * - store_verifier()/get_verifier(): PKCE验证码管理
 *
 * 关键流程
 * ========
 * 1. OAuth2登录流程:
 *    - 用户请求登录
 *    - 创建OAuth2客户端
 *    - 生成PKCE挑战和验证码
 *    - 构建授权URL并存储验证码
 *    - 重定向用户到OAuth2提供商的授权页面
 *
 * 2. OAuth2回调处理流程:
 *    - 接收OAuth2提供商的回调，包含授权码
 *    - 重定向到前端应用，传递授权码和状态
 *
 * 3. 用户信息获取流程:
 *    - 使用授权码和PKCE验证码交换访问令牌
 *    - 使用访问令牌从OAuth2提供商获取用户信息
 *    - 解析用户信息并返回标准化的用户对象
 *
 * 注意事项
 * ========
 * - 安全考虑: 实现PKCE以防止授权码拦截攻击
 * - 配置依赖: 需要正确配置OAuth2提供商的客户端ID、密钥和URL
 * - 提供商特异性: 不同的OAuth2提供商可能需要特定的处理逻辑
 * - 错误处理: OAuth2流程中的错误需要适当处理和记录
 * - 会话管理: 验证码需要临时存储并与会话关联
 * - 重定向安全: 确保重定向URL是预先配置的安全URL
 */

use anyhow::{Context, Result};
use axum::{
    extract::Query,
    response::{IntoResponse, Redirect, Response},
};
use chrono::Duration;
use http::StatusCode;
use oauth2::basic::BasicClient;
use oauth2::reqwest;
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use tracing::{error, trace};

use crate::config;
use crate::helpers::errors::PrintFullError;
use crate::storage::{get_async_redis_conn, redis_key};

type Client = BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

#[derive(Deserialize)]
struct ClerkUserinfo {
    pub email: String,
    pub email_verified: bool,
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct CallbackArgs {
    pub code: String,
    pub state: String,
}

#[derive(Serialize, Debug)]
pub struct User {
    pub email: String,
    pub email_verified: bool,
    pub external_id: String,
}

pub async fn login_handler() -> Response {
    let client = match get_client() {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e.full(), "Get OAuth2 client error");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let conf = config::get();

    let mut request = client.authorize_url(CsrfToken::new_random);

    for scope in &conf.user_authentication.oauth2.scopes {
        request = request.add_scope(Scope::new(scope.to_string()))
    }
    let (auth_url, csrf_token) = request.set_pkce_challenge(pkce_challenge).url();

    if let Err(e) = store_verifier(&csrf_token, &pkce_verifier).await {
        error!(error = %e.full(), "Store verifier error");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
    }

    Redirect::temporary(auth_url.as_str()).into_response()
}

pub async fn callback_handler(args: Query<CallbackArgs>) -> Response {
    let args: CallbackArgs = args.0;
    Redirect::permanent(&format!("/#/login?code={}&state={}", args.code, args.state))
        .into_response()
}

fn get_client() -> Result<Client> {
    let conf = config::get();

    if conf.user_authentication.enabled != "oauth2" {
        return Err(anyhow!("OAuth2 is not enabled"));
    }

    let client = BasicClient::new(ClientId::new(
        conf.user_authentication.oauth2.client_id.clone(),
    ))
    .set_client_secret(ClientSecret::new(
        conf.user_authentication.oauth2.client_secret.clone(),
    ))
    .set_auth_uri(AuthUrl::new(
        conf.user_authentication.oauth2.auth_url.clone(),
    )?)
    .set_token_uri(TokenUrl::new(
        conf.user_authentication.oauth2.token_url.clone(),
    )?)
    .set_redirect_uri(RedirectUrl::new(
        conf.user_authentication.oauth2.redirect_url.clone(),
    )?)
    .set_auth_type(match conf.user_authentication.oauth2.provider.as_ref() {
        "clerk" => AuthType::RequestBody, // clerk does not support BasicAuth
        _ => AuthType::BasicAuth,         // default oauth2 crate value
    });

    Ok(client)
}

pub async fn get_user(code: &str, state: &str) -> Result<User> {
    let state = oauth2::CsrfToken::new(state.to_string());
    let verifier = get_verifier(&state).await?;
    let client = get_client()?;

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let token = match client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .set_pkce_verifier(verifier)
        .request_async(&http_client)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow!(e.to_string()));
        }
    };
    let access_token = token.access_token().secret();

    let conf = config::get();
    let provider = conf.user_authentication.oauth2.provider.clone();
    let userinfo_url = conf.user_authentication.oauth2.userinfo_url.clone();

    match provider.as_ref() {
        "clerk" => get_clerk_user(access_token, &userinfo_url).await,
        _ => Err(anyhow!("Unsupported OAuth2 provider: {}", provider)),
    }
}

async fn get_clerk_user(token: &str, url: &str) -> Result<User> {
    let client = reqwest::Client::new();
    let auth_header = format!("Bearer {}", token);

    let resp: ClerkUserinfo = client
        .get(url)
        .header(AUTHORIZATION, auth_header)
        .send()
        .await?
        .json()
        .await?;

    Ok(User {
        email: resp.email,
        email_verified: resp.email_verified,
        external_id: resp.user_id,
    })
}

async fn store_verifier(
    token: &oauth2::CsrfToken,
    verifier: &oauth2::PkceCodeVerifier,
) -> Result<()> {
    trace!("Storing verifier");

    let key = redis_key(format!("auth:oauth2:{}", token.secret()));
    () = redis::cmd("PSETEX")
        .arg(key)
        .arg(Duration::try_minutes(5).unwrap().num_milliseconds())
        .arg(verifier.secret())
        .query_async(&mut get_async_redis_conn().await?)
        .await?;

    Ok(())
}

async fn get_verifier(token: &oauth2::CsrfToken) -> Result<oauth2::PkceCodeVerifier> {
    trace!("Getting verifier");
    let key = redis_key(format!("auth:oauth2:{}", token.secret()));
    let v: String = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut get_async_redis_conn().await?)
        .await
        .context("Get verifier")?;

    Ok(oauth2::PkceCodeVerifier::new(v))
}

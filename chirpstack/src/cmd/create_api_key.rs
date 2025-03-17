/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的API密钥创建工具，用于生成访问ChirpStack API的认证凭证。
 * 在ChirpStack系统中，API密钥是外部应用程序与ChirpStack进行安全通信的基础。
 * 该模块实现了创建具有管理员权限的API密钥的功能，这些密钥可用于自动化脚本、第三方集成或管理工具。
 *
 * 文件功能
 * ========
 * 该文件实现了一个命令行工具，用于创建新的API密钥并生成对应的JWT令牌。
 * 它与存储系统交互，创建API密钥记录，并使用系统配置的密钥生成JWT令牌。
 * 生成的API密钥和令牌将输出到控制台，供用户保存和使用。
 *
 * 主要组件
 * ========
 * - run(): 主函数，接收API密钥名称并创建新的API密钥
 * - API密钥创建：在存储系统中创建新的API密钥记录
 * - JWT令牌生成：基于API密钥ID生成JWT认证令牌
 *
 * 关键流程
 * ========
 * 1. 获取系统配置
 * 2. 初始化存储系统
 * 3. 创建具有管理员权限的API密钥记录
 * 4. 使用系统密钥为该API密钥生成JWT令牌
 * 5. 将API密钥ID和JWT令牌输出到控制台
 *
 * 注意事项
 * ========
 * - 生成的API密钥具有管理员权限，应妥善保管
 * - JWT令牌包含敏感信息，不应在不安全的环境中传输
 * - API密钥创建后无法查看其完整令牌，请确保在创建时保存
 * - 该工具应仅由系统管理员使用，并限制访问权限
 */

use anyhow::Result;

use crate::api::auth::claims;
use crate::config;
use crate::storage::api_key;

pub async fn run(name: &str) -> Result<()> {
    let conf = config::get();

    crate::storage::setup().await?;

    let key = api_key::create(api_key::ApiKey {
        name: name.to_string(),
        is_admin: true,
        ..Default::default()
    })
    .await?;

    let token = claims::AuthClaim::new_for_api_key(&key.id).encode(conf.api.secret.as_ref())?;

    println!("id: {}", key.id);
    println!("token: {}", token);

    Ok(())
}

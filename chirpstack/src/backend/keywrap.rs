/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的密钥加密包装功能实现，用于安全地传输和存储LoRaWAN会话密钥。
 * 在LoRaWAN后端接口规范中，密钥加密包装(Key Encryption Key, KEK)是保护会话密钥在不同网络组件间传输的机制。
 * 该模块实现了密钥的加密和解密功能，确保敏感的会话密钥在传输过程中得到保护。
 *
 * 文件功能
 * ========
 * 该文件实现了LoRaWAN后端接口规范中定义的密钥加密包装功能。
 * 它提供了两个主要功能：解包装(unwrap)密钥和包装(wrap)密钥，用于处理与Join Server和漫游服务器的安全通信。
 * 这些功能使ChirpStack能够安全地处理从外部系统接收的加密密钥，以及向外部系统发送加密的密钥。
 *
 * 主要组件
 * ========
 * - unwrap(): 解密从外部系统接收的加密密钥
 * - wrap(): 使用指定的KEK标签加密密钥以便安全传输
 * - 密钥查找：根据KEK标签查找相应的加密密钥
 *
 * 关键流程
 * ========
 * 1. 解包装(unwrap)流程：
 *    - 接收包含加密密钥的KeyEnvelope
 *    - 根据KEK标签查找相应的加密密钥
 *    - 使用找到的密钥解密会话密钥
 *    - 返回解密后的AES128Key
 * 
 * 2. 包装(wrap)流程：
 *    - 接收要加密的AES128Key和KEK标签
 *    - 根据KEK标签查找相应的加密密钥
 *    - 使用找到的密钥加密会话密钥
 *    - 返回包含加密密钥的KeyEnvelope
 *
 * 注意事项
 * ========
 * - 密钥管理是安全关键的功能，必须谨慎实现和使用
 * - KEK标签必须在系统配置中正确定义，否则加密/解密将失败
 * - 在生产环境中，应定期轮换KEK以增强安全性
 * - 密钥操作应记录在日志中，但不应记录密钥本身
 * - 该模块的正确功能对于设备加入和漫游功能至关重要
 */

use anyhow::Result;
use tracing::trace;

use crate::config;
use backend::KeyEnvelope;
use lrwn::AES128Key;

pub fn unwrap(ke: &KeyEnvelope) -> Result<AES128Key> {
    // Nothing to unwrap
    if ke.kek_label.is_empty() {
        return Ok(AES128Key::from_slice(&ke.aes_key)?);
    }

    trace!(kek_label = %ke.kek_label, "Unwrapping AES key");
    let conf = config::get();

    for kek in &conf.keks {
        if kek.label == ke.kek_label {
            let key = ke.unwrap(&kek.kek.to_bytes())?;
            return Ok(AES128Key::from_bytes(key));
        }
    }

    Err(anyhow!("KEK label {} does not exist", ke.kek_label))
}

pub fn wrap(label: &str, key: AES128Key) -> Result<KeyEnvelope> {
    if label.is_empty() {
        return KeyEnvelope::new("", None, &key.to_bytes());
    }

    let conf = config::get();
    for kek in &conf.keks {
        if kek.label == *label {
            return KeyEnvelope::new(label, Some(&kek.kek.to_bytes()), &key.to_bytes());
        }
    }

    Err(anyhow!("KEK label {} does not exist", label))
}

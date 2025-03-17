/**
 * @module helpers/tls
 * 
 * @description
 * 
 * # 模块概述
 * 本模块提供了TLS（传输层安全）相关的辅助功能，用于处理证书和密钥的加载、解析和转换。
 * 这些功能对于ChirpStack系统中的安全通信至关重要，包括与网关、应用服务器和其他外部系统的通信。
 * 
 * # 文件功能
 * - 加载和管理TLS根证书
 * - 读取和解析证书文件
 * - 加载和转换私钥
 * - 支持不同格式的私钥转换为PKCS#8格式
 * 
 * # 主要组件
 * - get_root_certs函数：获取系统根证书，并可选择添加自定义CA证书
 * - load_cert函数：从文件加载证书
 * - load_key函数：从文件加载私钥
 * - private_key_to_pkcs8函数：将不同格式的私钥转换为PKCS#8格式
 * 
 * # 关键流程
 * - 证书加载流程：
 *   1. 读取证书文件内容
 *   2. 解析PEM格式的证书
 *   3. 转换为rustls可用的证书格式
 * 
 * - 私钥处理流程：
 *   1. 读取私钥文件内容
 *   2. 检测私钥格式（RSA、EC或PKCS#8）
 *   3. 必要时将私钥转换为PKCS#8格式
 *   4. 转换为rustls可用的私钥格式
 * 
 * # 重要考虑事项
 * - 证书和私钥的安全存储和处理对系统安全至关重要
 * - 支持多种私钥格式以提高兼容性
 * - 错误处理应提供有用的上下文信息以便于调试
 * - 需要考虑不同操作系统的证书存储差异
 * - 使用异步IO操作以提高性能
 */

use std::fs::File;
use std::io::BufReader;

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::fs;

// Return root certificates, optionally with the provided ca_file appended.
pub fn get_root_certs(ca_file: Option<String>) -> Result<rustls::RootCertStore> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        roots.add(cert)?;
    }

    if let Some(ca_file) = &ca_file {
        let f = File::open(ca_file).context("Open CA certificate")?;
        let mut reader = BufReader::new(f);
        let certs = rustls_pemfile::certs(&mut reader);
        for cert in certs.flatten() {
            roots.add(cert)?;
        }
    }

    Ok(roots)
}

pub async fn load_cert(cert_file: &str) -> Result<Vec<CertificateDer<'static>>> {
    let cert_s = fs::read_to_string(cert_file)
        .await
        .context("Read TLS certificate")?;
    let mut cert_b = cert_s.as_bytes();
    let certs = rustls_pemfile::certs(&mut cert_b);
    let mut out = Vec::new();
    for cert in certs {
        out.push(cert?.into_owned());
    }
    Ok(out)
}

pub async fn load_key(key_file: &str) -> Result<PrivateKeyDer<'static>> {
    let key_s = fs::read_to_string(key_file)
        .await
        .context("Read private key")?;
    let key_s = private_key_to_pkcs8(&key_s)?;
    let mut key_b = key_s.as_bytes();
    let mut keys = rustls_pemfile::pkcs8_private_keys(&mut key_b);
    if let Some(key) = keys.next() {
        match key {
            Ok(v) => return Ok(PrivateKeyDer::Pkcs8(v.clone_key())),
            Err(e) => {
                return Err(anyhow!("Error parsing private key, error: {}", e));
            }
        }
    }

    Err(anyhow!("No private key found"))
}

pub fn private_key_to_pkcs8(pem: &str) -> Result<String> {
    if pem.contains("RSA PRIVATE KEY") {
        use rsa::{
            pkcs1::DecodeRsaPrivateKey,
            pkcs8::{EncodePrivateKey, LineEnding},
            RsaPrivateKey,
        };

        let pkey = RsaPrivateKey::from_pkcs1_pem(pem).context("Read RSA PKCS#1")?;
        let pkcs8_pem = pkey.to_pkcs8_pem(LineEnding::default())?;
        Ok(pkcs8_pem.as_str().to_owned())
    } else if pem.contains("EC PRIVATE KEY") {
        use elliptic_curve::{
            pkcs8::{EncodePrivateKey, LineEnding},
            SecretKey,
        };

        // We assume it is a P256 based secret-key, which is the most popular curve.
        // Attempting to decode it as P256 is still better than just failing to read it.
        let pkey: SecretKey<p256::NistP256> =
            SecretKey::from_sec1_pem(pem).context("Read EC SEC1")?;
        let pkcs8_pem = pkey.to_pkcs8_pem(LineEnding::default())?;
        Ok(pkcs8_pem.as_str().to_owned())
    } else {
        Ok(pem.to_string())
    }
}

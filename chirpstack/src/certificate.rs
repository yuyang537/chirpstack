/**
 * @module certificate
 * 
 * @description
 * 
 * # 模块概述
 * 本模块负责ChirpStack系统中的证书管理功能，主要用于生成和管理TLS证书，
 * 包括网关和应用程序的客户端证书。这些证书用于确保ChirpStack与网关和应用程序之间的安全通信。
 * 
 * # 文件功能
 * - 生成客户端证书，用于网关和应用程序的安全认证
 * - 读取和解析CA证书和密钥
 * - 为网关和应用程序提供基于其唯一标识符的证书生成功能
 * - 管理证书的有效期和签名算法
 * 
 * # 主要组件
 * - gen_client_cert：生成客户端证书的通用函数
 * - get_ca_cert：获取CA证书和密钥的函数
 * - client_cert_for_gateway_id：为特定网关ID生成客户端证书
 * - client_cert_for_application_id：为特定应用程序ID生成客户端证书
 * 
 * # 关键流程
 * - 读取配置中指定的CA证书和密钥
 * - 根据网关ID或应用程序ID生成唯一的客户端证书
 * - 设置证书的有效期、密钥用途和扩展密钥用途
 * - 返回生成的证书、私钥和CA证书
 * 
 * # 重要考虑事项
 * - 证书的安全性对于整个系统的安全至关重要
 * - 证书有效期通过配置文件设置，需要定期更新
 * - 支持不同的签名算法，确保与各种客户端的兼容性
 * - 证书生成是异步操作，支持高并发处理
 */

use std::time::SystemTime;

use anyhow::{Context, Result};
use rcgen::{
    Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose,
    SignatureAlgorithm,
};
use tokio::fs;
use uuid::Uuid;

use crate::config;
use crate::helpers::tls::private_key_to_pkcs8;
use lrwn::EUI64;

fn gen_client_cert(
    id: &str,
    not_before: SystemTime,
    not_after: SystemTime,
    issuer: &Certificate,
    issuer_key: &KeyPair,
) -> Result<(Certificate, KeyPair)> {
    let mut params = CertificateParams::new(vec![id.to_string()])?;
    params
        .distinguished_name
        .push(DnType::CommonName, id.to_string());
    params.use_authority_key_identifier_extension = true;
    params.not_before = not_before.into();
    params.not_after = not_after.into();
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ClientAuth);

    let kp = KeyPair::generate()?;
    Ok((params.signed_by(&kp, issuer, issuer_key)?, kp))
}

async fn get_ca_cert(ca_cert_file: &str, ca_key_file: &str) -> Result<(Certificate, KeyPair)> {
    let ca_cert_s = fs::read_to_string(ca_cert_file)
        .await
        .context("Read gateway ca_cert")?;
    let ca_key_s = fs::read_to_string(ca_key_file)
        .await
        .context("Read gateway ca_key")?;
    let ca_key_s = private_key_to_pkcs8(&ca_key_s)?;
    let ca_key_algo = read_algo(&ca_cert_s)?;

    let ca_key =
        KeyPair::from_pem_and_sign_algo(&ca_key_s, ca_key_algo).context("Parse gateway CA key")?;
    let params =
        CertificateParams::from_ca_cert_pem(&ca_cert_s).context("Parse gateway CA certificate")?;

    Ok((params.self_signed(&ca_key)?, ca_key))
}

// This returns the CA, certificate and private-key as PEM encoded strings.
pub async fn client_cert_for_gateway_id(
    gateway_id: &EUI64,
) -> Result<(SystemTime, String, String, String)> {
    let conf = config::get();
    let (ca_cert, ca_key) = get_ca_cert(&conf.gateway.ca_cert, &conf.gateway.ca_key)
        .await
        .context("Get CA cert")?;
    let not_before = SystemTime::now();
    let not_after = SystemTime::now() + conf.gateway.client_cert_lifetime;
    let (gw_cert, gw_key) = gen_client_cert(
        &gateway_id.to_string(),
        not_before,
        not_after,
        &ca_cert,
        &ca_key,
    )
    .context("Generate client certificate")?;

    Ok((
        not_after,
        ca_cert.pem(),
        gw_cert.pem(),
        gw_key.serialize_pem(),
    ))
}

pub async fn client_cert_for_application_id(
    application_id: &Uuid,
) -> Result<(SystemTime, String, String, String)> {
    let conf = config::get();
    let (ca_cert, ca_key) = get_ca_cert(
        &conf.integration.mqtt.client.ca_cert,
        &conf.integration.mqtt.client.ca_key,
    )
    .await?;
    let not_before = SystemTime::now();
    let not_after = SystemTime::now() + conf.integration.mqtt.client.client_cert_lifetime;
    let (app_cert, app_key) = gen_client_cert(
        &application_id.to_string(),
        not_before,
        not_after,
        &ca_cert,
        &ca_key,
    )?;

    Ok((
        not_after,
        ca_cert.pem(),
        app_cert.pem(),
        app_key.serialize_pem(),
    ))
}

// we are using String here, because else we run into lifetime issues.
fn read_algo(cert: &str) -> Result<&'static SignatureAlgorithm> {
    let cert = pem::parse(cert).context("Parse PEM")?;
    let (_remainder, x509) =
        x509_parser::parse_x509_certificate(cert.contents()).context("Parse x509")?;

    let alg_oid = x509
        .signature_algorithm
        .algorithm
        .iter()
        .ok_or_else(|| anyhow!("Parse certificate error"))?
        .collect::<Vec<_>>();
    Ok(SignatureAlgorithm::from_oid(&alg_oid)?)
}

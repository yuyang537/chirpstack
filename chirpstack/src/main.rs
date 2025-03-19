#![recursion_limit = "256"]

/**
 * @module main
 * 
 * @description
 * 
 * # 模块概述
 * 本文件是ChirpStack LoRaWAN网络服务器的主入口点，负责初始化系统、处理命令行参数、
 * 设置日志记录，并启动各个服务组件。ChirpStack是一个开源的LoRaWAN网络服务器，
 * 提供了完整的LoRaWAN网络后端功能。
 * 
 * # 文件功能
 * - 定义程序的入口点和命令行接口
 * - 导入和组织所有模块
 * - 处理命令行参数和子命令
 * - 初始化配置、日志和数据库连接
 * - 启动各个服务组件
 * 
 * # 主要组件
 * - Cli结构体：定义命令行参数和子命令
 * - Commands枚举：定义支持的子命令
 * - main函数：程序入口点，负责初始化和启动服务
 * - 各个功能模块的导入声明
 * 
 * # 关键流程
 * - 解析命令行参数
 * - 加载配置文件
 * - 设置日志记录
 * - 初始化数据库连接
 * - 根据命令执行相应操作或启动服务
 * - 处理信号以优雅地关闭服务
 * 
 * # 重要考虑事项
 * - 作为系统入口点，负责协调各个组件的初始化和启动
 * - 支持多种子命令，用于不同的管理和维护任务
 * - 配置文件路径通过命令行参数指定
 * - 服务的优雅启动和关闭对系统稳定性至关重要
 */

// Required by rust::table macro.

#[macro_use]
extern crate lazy_static;
extern crate diesel_migrations;
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate anyhow;

use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::filter;

use lrwn::EUI64;

// KLEE符号执行支持
#[cfg(feature = "klee")]
extern crate klee_sys;
#[cfg(feature = "klee")]
use klee_sys::{klee_assume, klee_make_symbolic};

mod adr;
mod api;
mod backend;
mod certificate;
mod cmd;
mod codec;
mod config;
mod devaddr;
mod downlink;
mod gateway;
mod gpstime;
mod helpers;
mod integration;
mod maccommand;
mod monitoring;
mod region;
mod sensitivity;
mod storage;
mod stream;
#[cfg(test)]
mod test;
mod uplink;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration directory
    #[arg(short, long, value_name = "DIR")]
    config: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the configuration template
    Configfile {},

    /// Print the device-session for debugging
    PrintDs {
        /// Device EUI
        #[arg(long, value_name = "DEV_EUI")]
        dev_eui: String,
    },

    /// Import lorawan-device-profiles repository.
    ImportLorawanDeviceProfiles {
        /// Path to repository root.
        #[arg(short, long, value_name = "DIR")]
        dir: String,
    },

    /// Import legacy lorawan-devices repository.
    ImportLegacyLorawanDevicesRepository {
        /// Path to repository root.
        #[arg(short, long, value_name = "DIR")]
        dir: String,
    },

    /// Create global API key.
    CreateApiKey {
        /// Name.
        #[arg(short, long, value_name = "NAME")]
        name: String,
    },

    /// Migrate device-sessions from Redis to PostgreSQL.
    MigrateDeviceSessionsToPostgres {},
    
    /// Run with KLEE symbolic execution
    #[cfg(feature = "klee")]
    KleeSymbolicExecution {},
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "klee")]
    {
        // 如果启用了KLEE特性，则运行符号执行
        run_klee_symbolic_execution().await?;
    }
    #[cfg(not(feature = "klee"))]
    {
        let cli = Cli::parse();
        config::load(Path::new(&cli.config))?;

        let conf = config::get();
        let filter = filter::Targets::new().with_targets(vec![
            ("chirpstack", Level::from_str(&conf.logging.level).unwrap()),
            ("backend", Level::from_str(&conf.logging.level).unwrap()),
            ("lrwn", Level::from_str(&conf.logging.level).unwrap()),
        ]);

        if conf.logging.json {
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().json())
                .with(filter)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer())
                .with(filter)
                .init();
        }

        match &cli.command {
            Some(Commands::Configfile {}) => cmd::configfile::run(),
            Some(Commands::PrintDs { dev_eui }) => {
                let dev_eui = EUI64::from_str(dev_eui).unwrap();
                cmd::print_ds::run(&dev_eui).await.unwrap();
            }
            Some(Commands::ImportLorawanDeviceProfiles { dir }) => {
                cmd::import_lorawan_device_profiles::run(Path::new(&dir))
                    .await
                    .unwrap()
            }
            Some(Commands::ImportLegacyLorawanDevicesRepository { dir }) => {
                cmd::import_legacy_lorawan_devices_repository::run(Path::new(&dir))
                    .await
                    .unwrap()
            }
            Some(Commands::CreateApiKey { name }) => cmd::create_api_key::run(name).await?,
            Some(Commands::MigrateDeviceSessionsToPostgres {}) => cmd::migrate_ds_to_pg::run().await?,
            #[cfg(feature = "klee")]
            Some(Commands::KleeSymbolicExecution {}) => run_klee_symbolic_execution().await?,
            None => cmd::root::run().await?,
        }
    }

    Ok(())
}

#[cfg(feature = "klee")]
async fn run_klee_symbolic_execution() -> Result<()> {
    use std::mem::size_of;
    use tracing::info;
    
    // 设置默认配置
    let config_path = Path::new("./configuration");
    config::load(config_path)?;
    
    info!("开始KLEE符号执行分析");
    
    // 创建符号输入
    let mut symbolic_dev_eui = [0u8; 8]; // DevEUI是8字节
    let mut symbolic_app_key = [0u8; 16]; // AppKey是16字节
    let mut symbolic_nwk_key = [0u8; 16]; // NwkKey是16字节
    let mut symbolic_payload = [0u8; 256]; // 上行数据包负载
    let mut symbolic_payload_len: usize = 0;
    
    // 使用KLEE创建符号变量
    unsafe {
        klee_make_symbolic(
            symbolic_dev_eui.as_mut_ptr() as *mut libc::c_void,
            size_of::<[u8; 8]>(),
            b"symbolic_dev_eui\0".as_ptr() as *const libc::c_char,
        );
        
        klee_make_symbolic(
            symbolic_app_key.as_mut_ptr() as *mut libc::c_void,
            size_of::<[u8; 16]>(),
            b"symbolic_app_key\0".as_ptr() as *const libc::c_char,
        );
        
        klee_make_symbolic(
            symbolic_nwk_key.as_mut_ptr() as *mut libc::c_void,
            size_of::<[u8; 16]>(),
            b"symbolic_nwk_key\0".as_ptr() as *const libc::c_char,
        );
        
        klee_make_symbolic(
            symbolic_payload.as_mut_ptr() as *mut libc::c_void,
            size_of::<[u8; 256]>(),
            b"symbolic_payload\0".as_ptr() as *const libc::c_char,
        );
        
        klee_make_symbolic(
            &mut symbolic_payload_len as *mut usize as *mut libc::c_void,
            size_of::<usize>(),
            b"symbolic_payload_len\0".as_ptr() as *const libc::c_char,
        );
        
        // 添加约束：payload长度不能超过256字节
        klee_assume((symbolic_payload_len <= 256) as i32);
        klee_assume((symbolic_payload_len > 0) as i32);
    }
    
    // 转换为ChirpStack使用的类型
    let dev_eui = EUI64::from_slice(&symbolic_dev_eui)?;
    let app_key = lrwn::AES128Key::from_slice(&symbolic_app_key)?;
    let nwk_key = lrwn::AES128Key::from_slice(&symbolic_nwk_key)?;
    
    // 初始化必要的服务
    storage::setup().await?;
    region::setup()?;
    backend::setup().await?;
    adr::setup().await?;
    integration::setup().await?;
    gateway::backend::setup().await?;
    downlink::setup().await;
    
    // 运行符号执行分析
    // 这里我们将调用关键的安全敏感函数进行分析
    analyze_security_sensitive_operations(dev_eui, app_key, nwk_key, &symbolic_payload[..symbolic_payload_len]).await?;
    
    Ok(())
}

#[cfg(feature = "klee")]
async fn analyze_security_sensitive_operations(
    dev_eui: EUI64,
    app_key: lrwn::AES128Key,
    nwk_key: lrwn::AES128Key,
    payload: &[u8],
) -> Result<()> {
    use lrwn::{MACVersion, MType, Major, MHDR, PhyPayload, Payload as LrwnPayload, MACPayload, FHDR, DevAddr, FCtrl};
    use tracing::{info, error};
    use klee_sys::klee_assert;
    
    info!("分析安全敏感操作");
    
    // 1. 分析MIC验证
    let mut phy = PhyPayload {
        mhdr: MHDR {
            m_type: MType::UnconfirmedDataUp,
            major: Major::LoRaWANR1,
        },
        payload: LrwnPayload::MACPayload(MACPayload {
            fhdr: FHDR {
                devaddr: DevAddr::from_be_bytes([1, 2, 3, 4]),
                f_ctrl: FCtrl::default(),
                f_cnt: 0,
                f_opts: lrwn::MACCommandSet::new(vec![]),
            },
            f_port: Some(1),
            frm_payload: Some(lrwn::FRMPayload::Raw(payload.to_vec())),
        }),
        mic: None,
    };
    
    // 设置MIC
    #[cfg(feature = "crypto")]
    {
        // 设置上行数据MIC
        phy.set_uplink_data_mic(
            MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &nwk_key,
            &nwk_key,
        )?;
        
        let original_mic = phy.mic.unwrap();
        
        // 验证MIC
        let mic_valid = phy.validate_uplink_data_mic(
            MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &nwk_key,
            &nwk_key,
        )?;
        
        // 断言：正确设置的MIC应该验证通过
        unsafe {
            klee_assert(mic_valid as i32);
        }
        
        // 篡改MIC
        if let Some(mic) = &mut phy.mic {
            mic[0] ^= 0x01;
        }
        
        // 验证篡改后的MIC
        let tampered_mic_valid = phy.validate_uplink_data_mic(
            MACVersion::LoRaWAN1_0,
            0,
            0,
            0,
            &nwk_key,
            &nwk_key,
        )?;
        
        // 断言：篡改后的MIC应该验证失败
        unsafe {
            klee_assert(!tampered_mic_valid as i32);
        }
        
        // 恢复原始MIC
        phy.mic = Some(original_mic);
    }
    
    // 2. 分析加密和解密
    #[cfg(feature = "crypto")]
    {
        // 加密帧负载
        phy.encrypt_frm_payload(&app_key)?;
        
        // 保存加密后的数据
        let _encrypted_data = if let LrwnPayload::MACPayload(ref pl) = phy.payload {
            if let Some(ref frm_payload) = pl.frm_payload {
                frm_payload.to_vec()?
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        
        // 解密帧负载
        phy.decrypt_frm_payload(&app_key)?;
        
        // 获取解密后的数据
        let decrypted_data = if let LrwnPayload::MACPayload(ref pl) = phy.payload {
            if let Some(ref frm_payload) = pl.frm_payload {
                frm_payload.to_vec()?
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        
        // 断言：解密后的数据应该与原始负载相同
        if decrypted_data.len() == payload.len() {
            for i in 0..payload.len() {
                unsafe {
                    klee_assert((decrypted_data[i] == payload[i]) as i32);
                }
            }
        }
    }
    
    // 3. 分析帧计数器
    {
        // 模拟帧计数器回滚攻击
        if let LrwnPayload::MACPayload(ref mut pl) = phy.payload {
            // 设置一个较大的帧计数器值
            pl.fhdr.f_cnt = 100;
        }
        
        // 设置MIC
        #[cfg(feature = "crypto")]
        {
            phy.set_uplink_data_mic(
                MACVersion::LoRaWAN1_0,
                0,
                0,
                0,
                &nwk_key,
                &nwk_key,
            )?;
            
            // 验证MIC
            let mic_valid = phy.validate_uplink_data_mic(
                MACVersion::LoRaWAN1_0,
                0,
                0,
                0,
                &nwk_key,
                &nwk_key,
            )?;
            
            unsafe {
                klee_assert(mic_valid as i32);
            }
        }
        
        // 现在尝试回滚帧计数器
        if let LrwnPayload::MACPayload(ref mut pl) = phy.payload {
            pl.fhdr.f_cnt = 50;
        }
        
        // 重新设置MIC以使其有效
        #[cfg(feature = "crypto")]
        {
            phy.set_uplink_data_mic(
                MACVersion::LoRaWAN1_0,
                0,
                0,
                0,
                &nwk_key,
                &nwk_key,
            )?;
        }
        
        // 这里我们不做断言，因为ChirpStack的帧计数器检查是在设备会话层面进行的
        // 在实际的设备会话处理中，应该会拒绝这种回滚的帧计数器
    }
    
    // 4. 调用uplink/join.rs中的分析函数
    uplink::join::analyze_join_request_with_klee(dev_eui, app_key.clone(), nwk_key.clone()).await?;
    
    // 5. 调用uplink/data.rs中的分析函数
    let dev_addr = DevAddr::from_be_bytes([1, 2, 3, 4]);
    uplink::data::analyze_uplink_data_with_klee(
        dev_eui,
        dev_addr,
        nwk_key.clone(),  // f_nwk_s_int_key
        nwk_key.clone(),  // s_nwk_s_int_key
        nwk_key.clone(),  // nwk_s_enc_key
        app_key.clone(),  // app_s_key
    ).await?;
    
    Ok(())
}

//! ChirpStack 物理载荷(PhyPayload)模块的KLEE分析
//! 
//! 这个文件作为PhyPayload模块的KLEE分析入口，用于检测该模块的安全问题
//! 
//! 编译命令：
//! RUSTFLAGS="--emit=llvm-ir -C debuginfo=2 -C opt-level=0 --cfg=feature=\"klee_analysis\" --cfg=feature=\"crypto\"" \
//! rustc -C panic=abort --crate-type=lib phy_payload_klee_entry.rs -o phy_payload_klee_entry.bc

#![cfg(feature = "klee_analysis")]

extern crate lrwn;

use std::os::raw::c_void;

// 导入需要分析的模块
use lrwn::{
    AES128Key,
    DevAddr,
    EUI64,
    MACVersion,
    MType,
    MHDR,
    Payload,
    PhyPayload,
    JoinType,
    MACPayload,
    FHDR,
    MACCommand,
    FRMPayload
};

// 定义KLEE外部函数
extern "C" {
    fn klee_make_symbolic(addr: *mut c_void, size: usize, name: *const u8);
    fn klee_assume(condition: u8);
    fn klee_assert(condition: u8);
    fn klee_report_error(file: *const u8, line: u32, message: *const u8, suffix: *const u8);
    fn klee_print_expr(expr: u8, name: *const u8);
}

// 封装KLEE函数，以便在Rust中使用
fn make_symbolic<T: Sized>(val: &mut T, name: &str) {
    unsafe {
        klee_make_symbolic(
            val as *mut T as *mut c_void,
            std::mem::size_of::<T>(),
            format!("{}\0", name).as_ptr(),
        );
    }
}

fn assert_true(condition: bool, message: &str) {
    if !condition {
        unsafe {
            klee_report_error(
                b"phy_payload_klee_entry.rs\0".as_ptr(),
                0,
                format!("{}\0", message).as_ptr(),
                b"assertion_fail\0".as_ptr(),
            );
        }
    }
}

fn print_expr(expr: bool, name: &str) {
    unsafe {
        klee_print_expr(expr as u8, format!("{}\0", name).as_ptr());
    }
}

// 创建符号化AES128密钥
fn make_symbolic_key(name: &str) -> AES128Key {
    let mut key_bytes = [0u8; 16];
    make_symbolic(&mut key_bytes, name);
    AES128Key::from_bytes(key_bytes)
}

// 创建符号化设备地址
fn make_symbolic_devaddr(name: &str) -> DevAddr {
    let mut addr_bytes = [0u8; 4];
    make_symbolic(&mut addr_bytes, name);
    DevAddr::from_be_bytes(addr_bytes)
}

// 创建符号化EUI64
fn make_symbolic_eui64(name: &str) -> EUI64 {
    let mut eui_bytes = [0u8; 8];
    make_symbolic(&mut eui_bytes, name);
    EUI64::from_be_bytes(eui_bytes)
}

// 测试1: 上行数据的MIC设置和验证
fn test_uplink_data_mic() {
    // 创建符号化密钥
    let f_nwk_s_int_key = make_symbolic_key("f_nwk_s_int_key");
    let s_nwk_s_int_key = make_symbolic_key("s_nwk_s_int_key");
    
    // 创建设备地址
    let dev_addr = make_symbolic_devaddr("dev_addr");
    
    // 创建上行MACPayload
    let mut mac_payload = MACPayload {
        fhdr: FHDR {
            devaddr: dev_addr,
            f_ctrl: Default::default(),
            f_cnt: 1, // 使用确定性值避免路径爆炸
            f_opts: vec![], // 简化，无选项
        },
        f_port: Some(1), // 使用FPort 1
        frm_payload: Some(FRMPayload::Raw(vec![0x01, 0x02, 0x03])), // 简单负载
    };
    
    // 创建物理载荷
    let mut phy_payload = PhyPayload {
        mhdr: MHDR {
            m_type: MType::UnconfirmedDataUp,
            major: 0,
        },
        payload: Payload::MACPayload(mac_payload.clone()),
        mic: None,
    };
    
    // 设置MIC (使用LoRaWAN 1.0)
    match phy_payload.set_uplink_data_mic(
        MACVersion::LoRaWAN1_0,
        0, // 确认帧计数器
        0, // 发送数据速率
        0, // 发送通道
        &f_nwk_s_int_key,
        &s_nwk_s_int_key,
    ) {
        Ok(_) => {
            // 现在应该有MIC了
            assert_true(phy_payload.mic.is_some(), "MIC应该被设置");
            
            // 验证MIC
            match phy_payload.validate_uplink_data_mic(
                MACVersion::LoRaWAN1_0,
                0, // 确认帧计数器
                0, // 发送数据速率
                0, // 发送通道
                &f_nwk_s_int_key,
                &s_nwk_s_int_key,
            ) {
                Ok(valid) => {
                    assert_true(valid, "MIC验证应该成功");
                    
                    // 更改key测试错误情况
                    let wrong_key = make_symbolic_key("wrong_key");
                    match phy_payload.validate_uplink_data_mic(
                        MACVersion::LoRaWAN1_0,
                        0,
                        0,
                        0,
                        &wrong_key,
                        &s_nwk_s_int_key,
                    ) {
                        Ok(valid) => {
                            // 使用错误的密钥时，验证不应该成功，除非两个密钥相同
                            if valid {
                                let mut keys_equal = true;
                                for i in 0..16 {
                                    if f_nwk_s_int_key.to_bytes()[i] != wrong_key.to_bytes()[i] {
                                        keys_equal = false;
                                        break;
                                    }
                                }
                                assert_true(keys_equal, "若验证成功，则密钥必须相同");
                            }
                        },
                        Err(_) => {
                            // 错误也是可接受的
                        }
                    }
                },
                Err(e) => {
                    assert_true(false, &format!("MIC验证失败: {:?}", e));
                }
            }
        },
        Err(e) => {
            assert_true(false, &format!("设置MIC失败: {:?}", e));
        }
    }
}

// 测试2: 加入请求的MIC设置和验证
fn test_join_request_mic() {
    // 创建符号化密钥
    let app_key = make_symbolic_key("app_key");
    
    // 创建加入请求载荷
    let mut phy_payload = PhyPayload {
        mhdr: MHDR {
            m_type: MType::JoinRequest,
            major: 0,
        },
        payload: Payload::JoinRequest(lrwn::payload::JoinRequestPayload {
            join_eui: make_symbolic_eui64("join_eui"),
            dev_eui: make_symbolic_eui64("dev_eui"),
            dev_nonce: 1234, // 使用确定性值
        }),
        mic: None,
    };
    
    // 设置MIC
    match phy_payload.set_join_request_mic(&app_key) {
        Ok(_) => {
            // 现在应该有MIC了
            assert_true(phy_payload.mic.is_some(), "MIC应该被设置");
            
            // 验证MIC
            match phy_payload.validate_join_request_mic(&app_key) {
                Ok(valid) => {
                    assert_true(valid, "MIC验证应该成功");
                    
                    // 更改key测试错误情况
                    let wrong_key = make_symbolic_key("wrong_app_key");
                    match phy_payload.validate_join_request_mic(&wrong_key) {
                        Ok(valid) => {
                            // 使用错误的密钥时，验证不应该成功，除非两个密钥相同
                            if valid {
                                let mut keys_equal = true;
                                for i in 0..16 {
                                    if app_key.to_bytes()[i] != wrong_key.to_bytes()[i] {
                                        keys_equal = false;
                                        break;
                                    }
                                }
                                assert_true(keys_equal, "若验证成功，则密钥必须相同");
                            }
                        },
                        Err(_) => {
                            // 错误也是可接受的
                        }
                    }
                },
                Err(e) => {
                    assert_true(false, &format!("MIC验证失败: {:?}", e));
                }
            }
        },
        Err(e) => {
            assert_true(false, &format!("设置MIC失败: {:?}", e));
        }
    }
}

// 测试3: 加入接受的加密和解密
fn test_join_accept_encryption() {
    // 创建符号化密钥
    let app_key = make_symbolic_key("app_key_for_join");
    
    // 创建加入接受载荷
    let join_nonce = [0x01, 0x02, 0x03];
    let net_id = [0x04, 0x05, 0x06];
    let devaddr = make_symbolic_devaddr("join_devaddr");
    
    let mut phy_payload = PhyPayload {
        mhdr: MHDR {
            m_type: MType::JoinAccept,
            major: 0,
        },
        payload: Payload::JoinAccept(lrwn::payload::JoinAcceptPayload {
            join_nonce: join_nonce,
            home_netid: net_id,
            devaddr: devaddr,
            dl_settings: Default::default(),
            rx_delay: 0,
            cf_list: None,
        }),
        mic: Some([0x01, 0x02, 0x03, 0x04]), // 设置一个初始MIC
    };
    
    // 加密载荷
    match phy_payload.encrypt_join_accept_payload(&app_key) {
        Ok(_) => {
            // 保存加密后的载荷
            let encrypted_payload = phy_payload.clone();
            
            // 解密载荷
            match phy_payload.decrypt_join_accept_payload(&app_key) {
                Ok(_) => {
                    // 使用错误的密钥解密
                    let wrong_key = make_symbolic_key("wrong_app_key_for_join");
                    let mut wrong_decrypt = encrypted_payload.clone();
                    
                    match wrong_decrypt.decrypt_join_accept_payload(&wrong_key) {
                        Ok(_) => {
                            // 如果解密成功，检查原始载荷和错误密钥解密的载荷
                            // 它们应该不同，除非两个密钥相同
                            
                            if let (Payload::JoinAccept(ref pl1), Payload::JoinAccept(ref pl2)) = 
                                   (phy_payload.payload, wrong_decrypt.payload) {
                                // 如果devaddr相同，说明解密是等效的
                                if pl1.devaddr == pl2.devaddr {
                                    // 这意味着两个密钥可能相同
                                    let mut keys_equal = true;
                                    for i in 0..16 {
                                        if app_key.to_bytes()[i] != wrong_key.to_bytes()[i] {
                                            keys_equal = false;
                                            break;
                                        }
                                    }
                                    assert_true(keys_equal, "若解密结果相同，则密钥必须相同");
                                }
                            }
                        },
                        Err(_) => {
                            // 错误也是可接受的，表示错误密钥无法正确解密
                        }
                    }
                },
                Err(e) => {
                    assert_true(false, &format!("解密加入接受载荷失败: {:?}", e));
                }
            }
        },
        Err(e) => {
            assert_true(false, &format!("加密加入接受载荷失败: {:?}", e));
        }
    }
}

// KLEE入口点
#[no_mangle]
pub extern "C" fn main() {
    // 运行所有测试
    test_uplink_data_mic();
    test_join_request_mic();
    test_join_accept_encryption();
} 
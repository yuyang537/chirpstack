/**
 * @module devaddr
 * 
 * @description
 * 
 * # 模块概述
 * 本模块负责ChirpStack系统中设备地址(DevAddr)的生成和管理。在LoRaWAN网络中，
 * DevAddr是一个32位的标识符，用于识别已激活的设备，是ABP(Activation By Personalization)
 * 和OTAA(Over-The-Air Activation)激活后设备通信的重要组成部分。
 * 
 * # 文件功能
 * - 提供随机DevAddr生成功能
 * - 确保生成的DevAddr符合配置的网络ID和前缀要求
 * - 支持多个DevAddr前缀的配置和随机选择
 * 
 * # 主要组件
 * - get_random_dev_addr：生成符合网络配置的随机DevAddr
 * 
 * # 关键流程
 * - 获取系统配置中的DevAddr前缀列表
 * - 如果未配置前缀，则使用网络ID(NetID)派生的默认前缀
 * - 随机选择一个前缀（如果配置了多个）
 * - 生成随机的32位地址
 * - 应用选定的前缀到随机地址
 * 
 * # 重要考虑事项
 * - DevAddr的前缀部分由LoRaWAN网络服务器的NetID决定
 * - 在大型网络中，应确保DevAddr的唯一性以避免冲突
 * - 测试环境中使用固定的DevAddr以确保测试的可重复性
 * - DevAddr是网络层安全的重要组成部分
 */

use rand::seq::SliceRandom;
use rand::RngCore;

use crate::config;
use lrwn::DevAddr;

pub fn get_random_dev_addr() -> DevAddr {
    let conf = config::get();
    let mut rng = rand::thread_rng();

    // Get configured DevAddr prefixes.
    let prefixes = if conf.network.dev_addr_prefixes.is_empty() {
        vec![conf.network.net_id.dev_addr_prefix()]
    } else {
        conf.network.dev_addr_prefixes.clone()
    };

    // Pick a random one (in case multiple prefixes are configured).
    let prefix = *prefixes.choose(&mut rng).unwrap();

    // Generate random DevAddr.
    let mut dev_addr: [u8; 4] = [0; 4];
    rng.fill_bytes(&mut dev_addr);
    #[cfg(test)]
    {
        dev_addr = [1, 2, 3, 4];
    }
    let mut dev_addr = DevAddr::from_be_bytes(dev_addr);

    // Set DevAddr prefix.
    dev_addr.set_dev_addr_prefix(prefix);
    dev_addr
}

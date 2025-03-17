/*
 * 模块概述
 * ========
 * Class B下行链路模块是ChirpStack LoRaWAN网络服务器下行链路处理框架的专用组件，负责实现
 * LoRaWAN Class B设备的下行通信机制。在LoRaWAN协议中，Class B设备通过同步网络信标(beacon)，
 * 在预定的时隙(ping slot)开启接收窗口，从而实现服务器主动下发数据的能力。
 * 
 * 该模块实现了LoRaWAN规范中定义的Class B下行机制，包括信标同步、ping时隙计算和下行调度。
 * 它与下行链路调度器和设备管理模块紧密集成，为ChirpStack提供了对Class B设备的完整支持。
 * 
 * 在LoRaWAN网络中，Class B模式是一种平衡电池寿命和下行链路延迟的重要机制，特别适用于
 * 需要服务器定期下发命令但又对功耗有要求的应用场景。本模块通过精确的时间计算和加密算法，
 * 确保下行数据能够在正确的时隙发送，从而被设备成功接收。
 *
 * 文件功能
 * ========
 * 本文件(classb.rs)实现了Class B设备下行通信的核心算法，提供了以下主要功能：
 * 1. 计算信标周期的起始时间
 * 2. 根据设备地址和ping周期计算ping偏移量
 * 3. 计算指定时间后的下一个可用ping时隙
 * 4. 提供Class B相关的时间常量和参数
 *
 * 主要组件
 * ========
 * - 常量定义:
 *   - BEACON_PERIOD: 信标周期长度(128秒)
 *   - BEACON_RESERVED: 信标保留时间(2.12秒)
 *   - BEACON_GUARD: 信标保护时间(3秒)
 *   - BEACON_WINDOW: 信标窗口时间(122.88秒)
 *   - PING_PERIOD_BASE: ping周期基数(2^12)
 *   - SLOT_LEN: 时隙长度(30毫秒)
 * 
 * - 函数:
 *   - get_beacon_start(): 计算给定时间所在信标周期的起始时间
 *   - get_ping_offset(): 计算设备的ping偏移量
 *   - get_next_ping_slot_after(): 计算指定时间后的下一个ping时隙
 *
 * 关键流程
 * ========
 * 1. 信标同步流程:
 *    - 根据GPS时间计算当前信标周期的起始时间
 *    - 信标周期固定为128秒，设备通过接收信标保持与网络的时间同步
 *
 * 2. Ping时隙计算流程:
 *    - 根据设备地址、信标时间和ping周期计算ping偏移量
 *    - 使用AES-128加密算法生成伪随机数，确保时隙分配的均匀性
 *    - 根据偏移量和ping周期确定设备的具体ping时隙
 *
 * 3. 下行调度流程:
 *    - 根据当前时间计算下一个可用的ping时隙
 *    - 将下行数据包调度到该时隙，确保设备能够接收
 *
 * 注意事项
 * ========
 * - 时间精度: Class B模式对时间同步要求高，需要精确的GPS时间源
 * - 加密算法: ping偏移量计算使用AES-128，需要正确实现
 * - 参数配置: ping周期(pingNb)影响设备功耗和下行延迟，需要根据应用需求合理配置
 * - 资源限制: 每个信标周期内的ping时隙数量有限，需要合理分配
 * - 协议合规: 实现必须严格遵循LoRaWAN规范中的Class B要求
 * - 网关同步: 网关也需要与信标同步，才能在正确的时间发送下行数据
 */

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::{Aes128, Block};
use anyhow::Result;
use chrono::Duration;
use tracing::debug;

use lrwn::DevAddr;

lazy_static! {
    static ref BEACON_PERIOD: Duration = Duration::try_seconds(128).unwrap();
    static ref BEACON_RESERVED: Duration = Duration::try_milliseconds(2120).unwrap();
    static ref BEACON_GUARD: Duration = Duration::try_seconds(3).unwrap();
    static ref BEACON_WINDOW: Duration = Duration::try_milliseconds(122880).unwrap();
    static ref PING_PERIOD_BASE: usize = 1 << 12;
    static ref SLOT_LEN: Duration = Duration::try_milliseconds(30).unwrap();
}

pub fn get_beacon_start(ts: Duration) -> Duration {
    Duration::try_seconds(ts.num_seconds() - (ts.num_seconds() % BEACON_PERIOD.num_seconds()))
        .unwrap_or_default()
}

pub fn get_ping_offset(beacon_ts: Duration, dev_addr: &DevAddr, ping_nb: usize) -> Result<usize> {
    if ping_nb == 0 {
        return Err(anyhow!("ping_nb must be > 0"));
    }

    let ping_period = *PING_PERIOD_BASE / ping_nb;
    let beacon_time = (beacon_ts.num_seconds() % (1 << 32)) as u32;

    let key_bytes: [u8; 16] = [0x00; 16];
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes128::new(key);

    let mut b: [u8; 16] = [0x00; 16];
    b[0..4].clone_from_slice(&beacon_time.to_le_bytes());
    b[4..8].clone_from_slice(&dev_addr.to_le_bytes());

    let mut block = Block::clone_from_slice(&b);
    cipher.encrypt_block(&mut block);
    let rand = block.as_slice();

    Ok(((rand[0] as usize) + ((rand[1] as usize) * 256)) % ping_period)
}

pub fn get_next_ping_slot_after(
    after_gps_epoch_ts: Duration,
    dev_addr: &DevAddr,
    ping_nb: usize,
) -> Result<Duration> {
    if ping_nb == 0 {
        return Err(anyhow!("ping_nb must be > 0"));
    }

    let mut beacon_start_ts = get_beacon_start(after_gps_epoch_ts);
    let ping_period = *PING_PERIOD_BASE / ping_nb;

    loop {
        let ping_offset = get_ping_offset(beacon_start_ts, dev_addr, ping_nb)?;
        for n in 0..ping_nb {
            let ping_slot_ts = beacon_start_ts
                + *BEACON_RESERVED
                + (*SLOT_LEN * ((ping_offset + n * ping_period) as i32));

            if ping_slot_ts > after_gps_epoch_ts {
                debug!(
                    dev_addr = %dev_addr,
                    beacon_start_time_s = beacon_start_ts.num_seconds(),
                    after_beacon_start_time_ms = (ping_slot_ts - beacon_start_ts).num_milliseconds(),
                    ping_offset_ms = ping_offset,
                    ping_slot_n = n,
                    ping_nb = ping_nb,
                    "Get next ping-slot timestamp"
                );
                return Ok(ping_slot_ts);
            }
        }

        beacon_start_ts += *BEACON_PERIOD;
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::gpstime::{ToDateTime, ToGpsTime};
    use chrono::{DateTime, TimeZone, Utc};

    #[test]
    fn test_get_beacon_start() {
        let gps_epoch_time: DateTime<Utc> = Utc.with_ymd_and_hms(1980, 1, 6, 0, 0, 0).unwrap();

        // For GPS epoch time
        let start_ts = get_beacon_start(gps_epoch_time.to_gps_time());
        assert_eq!(start_ts, Duration::zero());

        // For now
        let start_ts = get_beacon_start(Utc::now().to_gps_time());

        // > 0
        assert!(start_ts > Duration::zero());

        // Multiple of 128 seconds.
        assert_eq!(
            0,
            start_ts.num_nanoseconds().unwrap()
                % Duration::try_seconds(128)
                    .unwrap()
                    .num_nanoseconds()
                    .unwrap()
        );

        // Les than 128 seconds ago.
        let ts = start_ts.to_date_time();
        assert!(ts < Utc::now());
        assert!((Utc::now() - ts) < *BEACON_PERIOD);
    }

    #[test]
    fn test_get_ping_offset() {
        for k in 0..8 {
            let mut beacon_ts = Duration::zero();
            let ping_nb: usize = 1 << k;
            let ping_period = *PING_PERIOD_BASE / ping_nb;
            let dev_addr = DevAddr::from_be_bytes([0, 0, 0, 0]);

            for _ in 0..100000 {
                let offset = get_ping_offset(beacon_ts, &dev_addr, ping_nb).unwrap();
                assert!(offset < ping_period);
                beacon_ts += *BEACON_PERIOD;
            }
        }
    }

    #[test]
    fn test_get_next_ping_slot_after() {
        struct Test {
            after: Duration,
            dev_addr: DevAddr,
            ping_nb: usize,
            expected_ping_slot_ts: Duration,
        }

        let tests = vec![
            Test {
                after: Duration::zero(),
                dev_addr: DevAddr::from_be_bytes([0, 0, 0, 0]),
                ping_nb: 1,
                expected_ping_slot_ts: Duration::try_minutes(1).unwrap()
                    + Duration::try_seconds(14).unwrap()
                    + Duration::try_milliseconds(300).unwrap(),
            },
            Test {
                after: Duration::try_minutes(2).unwrap(),
                dev_addr: DevAddr::from_be_bytes([0, 0, 0, 0]),
                ping_nb: 1,
                expected_ping_slot_ts: Duration::try_minutes(3).unwrap()
                    + Duration::try_seconds(5).unwrap()
                    + Duration::try_milliseconds(620).unwrap(),
            },
            Test {
                after: Duration::zero(),
                dev_addr: DevAddr::from_be_bytes([0, 0, 0, 0]),
                ping_nb: 2,
                expected_ping_slot_ts: Duration::try_seconds(12).unwrap()
                    + Duration::try_milliseconds(860).unwrap(),
            },
            Test {
                after: Duration::try_seconds(13).unwrap(),
                dev_addr: DevAddr::from_be_bytes([0, 0, 0, 0]),
                ping_nb: 2,
                expected_ping_slot_ts: Duration::try_minutes(1).unwrap()
                    + Duration::try_seconds(14).unwrap()
                    + Duration::try_milliseconds(300).unwrap(),
            },
            Test {
                after: Duration::try_seconds(124).unwrap(),
                dev_addr: DevAddr::from_be_bytes([0, 0, 0, 0]),
                ping_nb: 128,
                expected_ping_slot_ts: Duration::try_minutes(2).unwrap()
                    + Duration::try_seconds(4).unwrap()
                    + Duration::try_milliseconds(220).unwrap(),
            },
        ];

        for tst in &tests {
            let ping_slot_ts =
                get_next_ping_slot_after(tst.after, &tst.dev_addr, tst.ping_nb).unwrap();
            assert_eq!(tst.expected_ping_slot_ts, ping_slot_ts);
        }
    }
}

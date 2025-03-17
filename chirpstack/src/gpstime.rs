/**
 * @module gpstime
 * 
 * @description
 * 
 * # 模块概述
 * 本模块提供GPS时间与UTC时间之间的转换功能。在LoRaWAN网络中，特别是Class B设备和某些
 * 精确定时操作中，需要使用GPS时间作为参考。GPS时间从1980年1月6日开始计算，与UTC时间
 * 存在闰秒差异，因此需要特殊的转换机制。
 * 
 * # 文件功能
 * - 定义GPS时间起点（1980年1月6日）
 * - 维护闰秒表，记录UTC时间中添加的闰秒
 * - 提供UTC时间转换为GPS时间的功能
 * - 提供GPS时间转换为UTC时间的功能
 * 
 * # 主要组件
 * - ToGpsTime trait：将UTC时间转换为GPS时间的接口
 * - ToDateTime trait：将GPS时间转换为UTC时间的接口
 * - GPS_EPOCH_TIME：GPS时间的起始点常量
 * - LEAP_SECONDS_TABLE：记录所有闰秒的表格
 * 
 * # 关键流程
 * - 计算UTC时间与GPS起始时间的差值
 * - 根据闰秒表调整时间差，考虑闰秒的影响
 * - 在两个时间系统之间进行精确转换
 * 
 * # 重要考虑事项
 * - GPS时间不考虑闰秒，而UTC时间会定期添加闰秒
 * - 闰秒表需要定期更新以反映新增的闰秒
 * - 时间转换在LoRaWAN Class B设备的ping slot计算中尤为重要
 * - 精确的时间同步对于某些LoRaWAN功能至关重要
 */

use chrono::{DateTime, Duration, TimeZone, Utc};

lazy_static! {
    static ref GPS_EPOCH_TIME: DateTime<Utc> = Utc.with_ymd_and_hms(1980, 1, 6, 0, 0, 0).unwrap();
    static ref LEAP_SECONDS_TABLE: Vec<(DateTime<Utc>, Duration)> = vec![
        (
            Utc.with_ymd_and_hms(1981, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1982, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1983, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1985, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1987, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1989, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1990, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1992, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1993, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1994, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1995, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1997, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(1998, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(2005, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(2008, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(2012, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(2015, 6, 30, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
        (
            Utc.with_ymd_and_hms(2016, 12, 31, 23, 59, 59).unwrap(),
            Duration::try_seconds(1).unwrap()
        ),
    ];
}

pub trait ToGpsTime {
    fn to_gps_time(&self) -> Duration;
}

pub trait ToDateTime {
    fn to_date_time(&self) -> DateTime<Utc>;
}

impl ToGpsTime for DateTime<Utc> {
    fn to_gps_time(&self) -> Duration {
        let mut offset = Duration::zero();
        for ls in LEAP_SECONDS_TABLE.iter() {
            if &ls.0 < self {
                offset += ls.1;
            }
        }

        self.signed_duration_since(*GPS_EPOCH_TIME) + offset
    }
}

impl ToDateTime for Duration {
    fn to_date_time(&self) -> DateTime<Utc> {
        let mut t = *GPS_EPOCH_TIME + *self;
        for ls in LEAP_SECONDS_TABLE.iter() {
            if ls.0 < t {
                t -= ls.1;
            }
        }
        t
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    struct Test {
        time: DateTime<Utc>,
        time_since_gps_epoch: Duration,
    }

    #[test]
    fn test() {
        let tests = vec![
            Test {
                time: *GPS_EPOCH_TIME,
                time_since_gps_epoch: Duration::zero(),
            },
            Test {
                time: Utc.with_ymd_and_hms(2010, 1, 28, 16, 36, 24).unwrap(),
                time_since_gps_epoch: Duration::try_seconds(948731799).unwrap(),
            },
            Test {
                time: Utc.with_ymd_and_hms(2025, 7, 14, 0, 0, 0).unwrap(),
                time_since_gps_epoch: Duration::try_seconds(1436486418).unwrap(),
            },
            Test {
                time: Utc.with_ymd_and_hms(2012, 6, 30, 23, 59, 59).unwrap(),
                time_since_gps_epoch: Duration::try_seconds(1025136014).unwrap(),
            },
            Test {
                time: Utc.with_ymd_and_hms(2012, 7, 1, 0, 0, 0).unwrap(),
                time_since_gps_epoch: Duration::try_seconds(1025136016).unwrap(),
            },
        ];

        for tst in tests {
            assert_eq!(tst.time_since_gps_epoch, tst.time.to_gps_time());
            assert_eq!(tst.time, tst.time_since_gps_epoch.to_date_time());
        }
    }
}

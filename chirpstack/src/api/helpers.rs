/*
 * 模块概述
 * ========
 * API辅助工具模块是ChirpStack LoRaWAN网络服务器的基础支持组件，提供了一系列工具函数和类型转换机制，
 * 用于在ChirpStack内部数据模型和API数据模型之间进行转换。该模块是连接内部业务逻辑和外部API接口的桥梁，
 * 确保数据在不同层次间的一致性和正确性。
 * 
 * 在LoRaWAN网络服务器中，需要处理多种不同的数据格式和协议版本。本模块通过提供统一的转换接口，
 * 简化了这些复杂性，使开发者能够专注于业务逻辑而非数据转换细节。同时，它也确保了API响应的一致性
 * 和符合LoRaWAN规范的数据表示。
 *
 * 文件功能
 * ========
 * 本文件(helpers.rs)实现了一系列类型转换特性和辅助函数，主要用于：
 * 1. 在ChirpStack内部数据类型和Protocol Buffers生成的API类型之间进行转换
 * 2. 处理LoRaWAN特定的数据类型，如区域参数、MAC版本、调制方式等
 * 3. 提供时间戳转换功能，在Chrono的DateTime和Protobuf的Timestamp之间转换
 * 4. 支持各种枚举类型的映射，确保API和内部表示的一致性
 *
 * 主要组件
 * ========
 * - FromProto<T>: 特性，定义从API类型到内部类型的转换
 * - ToProto<T>: 特性，定义从内部类型到API类型的转换
 * - 各种类型的转换实现:
 *   - 区域参数(Region)转换
 *   - MAC版本(MacVersion)转换
 *   - 区域参数修订版(RegParamsRevision)转换
 *   - 编解码器运行时(CodecRuntime)转换
 *   - 测量类型(MeasurementKind)转换
 *   - 聚合类型(Aggregation)转换
 *   - 消息类型(MType)转换
 *   - 多播组调度类型(MulticastGroupSchedulingType)转换
 *   - 中继模式激活(RelayModeActivation)转换
 *   - 设备类别(DeviceClass)转换
 *   - 排序字段(OrderBy)转换
 * - datetime_to_prost_timestamp: 将Chrono的DateTime转换为Protobuf的Timestamp
 *
 * 关键流程
 * ========
 * 1. API层接收到请求，包含Protocol Buffers格式的数据
 * 2. 使用FromProto特性将API数据转换为内部数据模型
 * 3. 业务逻辑处理内部数据模型
 * 4. 处理完成后，使用ToProto特性将内部结果转换回API格式
 * 5. 返回API响应给客户端
 *
 * 注意事项
 * ========
 * - 类型转换需要保持双向一致性，避免数据丢失或错误解释
 * - 对于枚举类型，需要处理未知值或未来可能添加的新值
 * - 时间戳转换需要注意时区和精度问题
 * - 某些LoRaWAN特定类型(如MAC版本)在不同版本间有细微差别，需要正确映射
 * - 添加新的API字段时，需要同步更新相应的转换逻辑
 * - 性能考虑：转换函数被频繁调用，应尽可能高效
 * - 对于Latest版本的枚举值，需要映射到当前支持的最新版本
 */

use chrono::{DateTime, Utc};

use crate::codec::Codec;
use crate::storage::fields::{MeasurementKind, MulticastGroupSchedulingType};
use crate::storage::{device, device::DeviceClass, gateway, metrics::Aggregation};
use chirpstack_api::{api, common};
use lrwn::region::{CommonName, MacVersion, Revision};

pub trait FromProto<T> {
    #[allow(clippy::wrong_self_convention)]
    fn from_proto(self) -> T;
}

pub trait ToProto<T> {
    fn to_proto(self) -> T;
}

impl FromProto<CommonName> for common::Region {
    fn from_proto(self) -> CommonName {
        match self {
            common::Region::Eu868 => CommonName::EU868,
            common::Region::Us915 => CommonName::US915,
            common::Region::Cn779 => CommonName::CN779,
            common::Region::Eu433 => CommonName::EU433,
            common::Region::Au915 => CommonName::AU915,
            common::Region::Cn470 => CommonName::CN470,
            common::Region::As923 => CommonName::AS923,
            common::Region::As9232 => CommonName::AS923_2,
            common::Region::As9233 => CommonName::AS923_3,
            common::Region::As9234 => CommonName::AS923_4,
            common::Region::Kr920 => CommonName::KR920,
            common::Region::In865 => CommonName::IN865,
            common::Region::Ru864 => CommonName::RU864,
            common::Region::Ism2400 => CommonName::ISM2400,
        }
    }
}

impl ToProto<common::Region> for CommonName {
    fn to_proto(self) -> common::Region {
        match self {
            CommonName::EU868 => common::Region::Eu868,
            CommonName::US915 => common::Region::Us915,
            CommonName::CN779 => common::Region::Cn779,
            CommonName::EU433 => common::Region::Eu433,
            CommonName::AU915 => common::Region::Au915,
            CommonName::CN470 => common::Region::Cn470,
            CommonName::AS923 => common::Region::As923,
            CommonName::AS923_2 => common::Region::As9232,
            CommonName::AS923_3 => common::Region::As9233,
            CommonName::AS923_4 => common::Region::As9234,
            CommonName::KR920 => common::Region::Kr920,
            CommonName::IN865 => common::Region::In865,
            CommonName::RU864 => common::Region::Ru864,
            CommonName::ISM2400 => common::Region::Ism2400,
        }
    }
}

impl FromProto<Revision> for common::RegParamsRevision {
    fn from_proto(self) -> Revision {
        match self {
            common::RegParamsRevision::A => Revision::A,
            common::RegParamsRevision::B => Revision::B,
            common::RegParamsRevision::Rp002100 => Revision::RP002_1_0_0,
            common::RegParamsRevision::Rp002101 => Revision::RP002_1_0_1,
            common::RegParamsRevision::Rp002102 => Revision::RP002_1_0_2,
            common::RegParamsRevision::Rp002103 => Revision::RP002_1_0_3,
            common::RegParamsRevision::Rp002104 => Revision::RP002_1_0_4,
        }
    }
}

impl ToProto<common::RegParamsRevision> for Revision {
    fn to_proto(self) -> common::RegParamsRevision {
        match self {
            Revision::A => common::RegParamsRevision::A,
            Revision::B => common::RegParamsRevision::B,
            Revision::RP002_1_0_0 => common::RegParamsRevision::Rp002100,
            Revision::RP002_1_0_1 => common::RegParamsRevision::Rp002101,
            Revision::RP002_1_0_2 => common::RegParamsRevision::Rp002102,
            Revision::RP002_1_0_3 => common::RegParamsRevision::Rp002103,
            Revision::RP002_1_0_4 | Revision::Latest => common::RegParamsRevision::Rp002104,
        }
    }
}

impl FromProto<MacVersion> for common::MacVersion {
    fn from_proto(self) -> MacVersion {
        match self {
            common::MacVersion::Lorawan100 => MacVersion::LORAWAN_1_0_0,
            common::MacVersion::Lorawan101 => MacVersion::LORAWAN_1_0_1,
            common::MacVersion::Lorawan102 => MacVersion::LORAWAN_1_0_2,
            common::MacVersion::Lorawan103 => MacVersion::LORAWAN_1_0_3,
            common::MacVersion::Lorawan104 => MacVersion::LORAWAN_1_0_4,
            common::MacVersion::Lorawan110 => MacVersion::LORAWAN_1_1_0,
        }
    }
}

impl ToProto<common::MacVersion> for MacVersion {
    fn to_proto(self) -> common::MacVersion {
        match self {
            MacVersion::LORAWAN_1_0_0 => common::MacVersion::Lorawan100,
            MacVersion::LORAWAN_1_0_1 => common::MacVersion::Lorawan101,
            MacVersion::LORAWAN_1_0_2 => common::MacVersion::Lorawan102,
            MacVersion::LORAWAN_1_0_3 => common::MacVersion::Lorawan103,
            MacVersion::LORAWAN_1_0_4 => common::MacVersion::Lorawan104,
            MacVersion::LORAWAN_1_1_0 | MacVersion::Latest => common::MacVersion::Lorawan110,
        }
    }
}

impl FromProto<lrwn::MACVersion> for common::MacVersion {
    fn from_proto(self) -> lrwn::MACVersion {
        match self {
            common::MacVersion::Lorawan100 => lrwn::MACVersion::LoRaWAN1_0,
            common::MacVersion::Lorawan101 => lrwn::MACVersion::LoRaWAN1_0,
            common::MacVersion::Lorawan102 => lrwn::MACVersion::LoRaWAN1_0,
            common::MacVersion::Lorawan103 => lrwn::MACVersion::LoRaWAN1_0,
            common::MacVersion::Lorawan104 => lrwn::MACVersion::LoRaWAN1_0,
            common::MacVersion::Lorawan110 => lrwn::MACVersion::LoRaWAN1_1,
        }
    }
}

impl ToProto<api::CodecRuntime> for Codec {
    fn to_proto(self) -> api::CodecRuntime {
        match self {
            Codec::NONE => api::CodecRuntime::None,
            Codec::CAYENNE_LPP => api::CodecRuntime::CayenneLpp,
            Codec::JS => api::CodecRuntime::Js,
        }
    }
}

impl FromProto<Codec> for api::CodecRuntime {
    fn from_proto(self) -> Codec {
        match self {
            api::CodecRuntime::None => Codec::NONE,
            api::CodecRuntime::CayenneLpp => Codec::CAYENNE_LPP,
            api::CodecRuntime::Js => Codec::JS,
        }
    }
}

impl ToProto<api::MeasurementKind> for MeasurementKind {
    fn to_proto(self) -> api::MeasurementKind {
        match self {
            MeasurementKind::UNKNOWN => api::MeasurementKind::Unknown,
            MeasurementKind::COUNTER => api::MeasurementKind::Counter,
            MeasurementKind::ABSOLUTE => api::MeasurementKind::Absolute,
            MeasurementKind::GAUGE => api::MeasurementKind::Gauge,
            MeasurementKind::STRING => api::MeasurementKind::String,
        }
    }
}

impl FromProto<MeasurementKind> for api::MeasurementKind {
    fn from_proto(self) -> MeasurementKind {
        match self {
            api::MeasurementKind::Unknown => MeasurementKind::UNKNOWN,
            api::MeasurementKind::Counter => MeasurementKind::COUNTER,
            api::MeasurementKind::Absolute => MeasurementKind::ABSOLUTE,
            api::MeasurementKind::Gauge => MeasurementKind::GAUGE,
            api::MeasurementKind::String => MeasurementKind::STRING,
        }
    }
}

impl ToProto<common::Aggregation> for Aggregation {
    fn to_proto(self) -> common::Aggregation {
        match self {
            Aggregation::MINUTE => common::Aggregation::Minute,
            Aggregation::HOUR => common::Aggregation::Hour,
            Aggregation::DAY => common::Aggregation::Day,
            Aggregation::MONTH => common::Aggregation::Month,
        }
    }
}

impl FromProto<Aggregation> for common::Aggregation {
    fn from_proto(self) -> Aggregation {
        match self {
            common::Aggregation::Minute => Aggregation::MINUTE,
            common::Aggregation::Hour => Aggregation::HOUR,
            common::Aggregation::Day => Aggregation::DAY,
            common::Aggregation::Month => Aggregation::MONTH,
        }
    }
}

impl ToProto<common::MType> for lrwn::MType {
    fn to_proto(self) -> common::MType {
        match self {
            lrwn::MType::JoinRequest => common::MType::JoinRequest,
            lrwn::MType::JoinAccept => common::MType::JoinAccept,
            lrwn::MType::UnconfirmedDataUp => common::MType::UnconfirmedDataUp,
            lrwn::MType::UnconfirmedDataDown => common::MType::UnconfirmedDataDown,
            lrwn::MType::ConfirmedDataUp => common::MType::ConfirmedDataUp,
            lrwn::MType::ConfirmedDataDown => common::MType::ConfirmedDataDown,
            lrwn::MType::RejoinRequest => common::MType::RejoinRequest,
            lrwn::MType::Proprietary => common::MType::Proprietary,
        }
    }
}

impl ToProto<api::MulticastGroupSchedulingType> for MulticastGroupSchedulingType {
    fn to_proto(self) -> api::MulticastGroupSchedulingType {
        match self {
            MulticastGroupSchedulingType::DELAY => api::MulticastGroupSchedulingType::Delay,
            MulticastGroupSchedulingType::GPS_TIME => api::MulticastGroupSchedulingType::GpsTime,
        }
    }
}

impl FromProto<MulticastGroupSchedulingType> for api::MulticastGroupSchedulingType {
    fn from_proto(self) -> MulticastGroupSchedulingType {
        match self {
            api::MulticastGroupSchedulingType::Delay => MulticastGroupSchedulingType::DELAY,
            api::MulticastGroupSchedulingType::GpsTime => MulticastGroupSchedulingType::GPS_TIME,
        }
    }
}

impl ToProto<api::RelayModeActivation> for lrwn::RelayModeActivation {
    fn to_proto(self) -> api::RelayModeActivation {
        match self {
            lrwn::RelayModeActivation::DisableRelayMode => {
                api::RelayModeActivation::DisableRelayMode
            }
            lrwn::RelayModeActivation::EnableRelayMode => api::RelayModeActivation::EnableRelayMode,
            lrwn::RelayModeActivation::Dynamic => api::RelayModeActivation::Dynamic,
            lrwn::RelayModeActivation::EndDeviceControlled => {
                api::RelayModeActivation::EndDeviceControlled
            }
        }
    }
}

impl FromProto<lrwn::RelayModeActivation> for api::RelayModeActivation {
    fn from_proto(self) -> lrwn::RelayModeActivation {
        match self {
            api::RelayModeActivation::DisableRelayMode => {
                lrwn::RelayModeActivation::DisableRelayMode
            }
            api::RelayModeActivation::EnableRelayMode => lrwn::RelayModeActivation::EnableRelayMode,
            api::RelayModeActivation::Dynamic => lrwn::RelayModeActivation::Dynamic,
            api::RelayModeActivation::EndDeviceControlled => {
                lrwn::RelayModeActivation::EndDeviceControlled
            }
        }
    }
}

impl ToProto<common::DeviceClass> for DeviceClass {
    fn to_proto(self) -> common::DeviceClass {
        match self {
            DeviceClass::A => common::DeviceClass::ClassA,
            DeviceClass::B => common::DeviceClass::ClassB,
            DeviceClass::C => common::DeviceClass::ClassC,
        }
    }
}

impl FromProto<device::OrderBy> for api::list_devices_request::OrderBy {
    fn from_proto(self) -> device::OrderBy {
        match self {
            Self::Name => device::OrderBy::Name,
            Self::DevEui => device::OrderBy::DevEui,
            Self::LastSeenAt => device::OrderBy::LastSeenAt,
            Self::DeviceProfileName => device::OrderBy::DeviceProfileName,
        }
    }
}

impl FromProto<gateway::OrderBy> for api::list_gateways_request::OrderBy {
    fn from_proto(self) -> gateway::OrderBy {
        match self {
            Self::Name => gateway::OrderBy::Name,
            Self::GatewayId => gateway::OrderBy::GatewayId,
            Self::LastSeenAt => gateway::OrderBy::LastSeenAt,
        }
    }
}

pub fn datetime_to_prost_timestamp(dt: &DateTime<Utc>) -> prost_types::Timestamp {
    let ts = dt.timestamp_nanos_opt().unwrap_or_default();

    prost_types::Timestamp {
        seconds: ts / 1_000_000_000,
        nanos: (ts % 1_000_000_000) as i32,
    }
}

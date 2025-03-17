/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的旧版设备库导入工具，用于从旧版LoRaWAN设备库导入设备配置。
 * 在ChirpStack系统中，该模块负责处理旧版格式的设备库数据，并将其转换为新系统可用的格式。
 * 它支持系统升级和迁移，确保用户可以无缝地从旧版设备库过渡到新版ChirpStack。
 *
 * 文件功能
 * ========
 * 该文件实现了一个命令行工具，用于从旧版LoRaWAN设备库目录导入设备配置文件。
 * 它解析旧版设备库的特定格式，包括厂商信息、设备信息、固件版本和配置文件等。
 * 导入的配置文件将作为设备配置模板存储在ChirpStack中，供用户在创建设备时使用。
 *
 * 主要组件
 * ========
 * - 数据结构：定义了旧版设备库的数据模型，包括Vendors、Vendor、Devices、Device等
 * - run(): 主函数，处理整个导入过程
 * - import_device(): 处理单个设备的导入
 * - 编解码器处理：解析和导入设备的编解码器配置
 *
 * 关键流程
 * ========
 * 1. 扫描指定目录，查找厂商和设备信息
 * 2. 解析设备配置文件，包括固件版本和配置文件
 * 3. 处理设备的编解码器配置
 * 4. 将配置转换为ChirpStack设备配置模板
 * 5. 将模板存储到ChirpStack系统中
 *
 * 注意事项
 * ========
 * - 导入过程依赖于旧版设备库的特定目录结构和文件格式
 * - 某些旧版配置可能需要手动调整才能在新系统中正常工作
 * - 编解码器脚本可能需要适配新系统的API
 * - 导入过程中的错误会被记录但不会中断整个过程
 * - 导入后应验证设备配置的正确性
 */

use std::collections::{HashMap, HashSet};
use std::fs::{read_to_string, File};
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use serde::Deserialize;
use tracing::{info, warn};

use crate::codec::Codec;
use crate::storage::{self, device_profile_template};
use lrwn::region::{CommonName, MacVersion, Revision};

#[derive(Deserialize, Default)]
#[serde(default)]
struct Vendors {
    pub vendors: Vec<Vendor>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Vendor {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Devices {
    #[serde(rename = "endDevices")]
    pub end_devices: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Device {
    pub name: String,
    pub description: String,
    #[serde(rename = "firmwareVersions")]
    pub firmware_versions: Vec<FirmwareVersion>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FirmwareVersion {
    pub version: String,
    pub profiles: HashMap<String, ProfileMeta>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ProfileMeta {
    #[serde(rename = "vendorID")]
    pub vendor_id: String,
    pub id: String,
    pub codec: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Profile {
    #[serde(rename = "vendorProfileID")]
    pub vendor_profile_id: String,
    #[serde(rename = "macVersion")]
    pub mac_version: String,
    #[serde(rename = "regionalParametersVersion")]
    pub region_parameters_version: String,
    #[serde(rename = "supportsJoin")]
    pub supports_join: bool,
    #[serde(rename = "rx1Delay")]
    pub rx1_delay: u8,
    #[serde(rename = "rx1DataRateOffset")]
    pub rx1_data_rate_offset: u8,
    #[serde(rename = "rx2DataRateIndex")]
    pub rx2_data_rate_index: u8,
    #[serde(rename = "rx2Frequency")]
    pub rx2_frequency: f64,
    #[serde(rename = "supportsClassB")]
    pub supports_class_b: bool,
    #[serde(rename = "classBTimeout")]
    pub class_b_timeout: usize, // seconds
    #[serde(rename = "pingSlotPeriod")]
    pub ping_slot_period: usize,
    #[serde(rename = "pingSlotDataRateIndex")]
    pub ping_slot_data_rate_index: u8,
    #[serde(rename = "pingSlotFrequency")]
    pub ping_slot_frequency: f64,
    #[serde(rename = "supportsClassC")]
    pub supports_class_c: bool,
    #[serde(rename = "classCTimeout")]
    pub class_c_timeout: usize,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct JsCodec {
    #[serde(rename = "uplinkDecoder")]
    pub uplink_decoder: Option<CodecFunction>,
    #[serde(rename = "downlinkEncoder")]
    pub downlink_encoder: Option<CodecFunction>,
    #[serde(rename = "downlinkDecoder")]
    pub downlink_decoder: Option<CodecFunction>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CodecFunction {
    #[serde(rename = "fileName")]
    pub filename: String,
}

pub async fn run(dir: &Path) -> Result<()> {
    storage::setup().await?;

    let vendor_index_yml = dir.join("vendor").join("index.yaml");
    info!(path = ?vendor_index_yml, "Reading vendor index file");

    let vendors: Vendors = serde_yaml::from_reader(File::open(&vendor_index_yml)?)?;
    for vendor in &vendors.vendors {
        if vendor.id == "example" {
            continue;
        }

        info!(vendor_id = %vendor.id, vendor_name = %vendor.name, "Found vendor");
        let vendor_dir = dir.join("vendor").join(&vendor.id);

        let devices_index_yml = vendor_dir.join("index.yaml");
        info!(path = ?devices_index_yml, "Reading devices index file");
        let devices: Devices = serde_yaml::from_reader(match File::open(&devices_index_yml) {
            Ok(v) => v,
            Err(e) => {
                warn!(path = ?devices_index_yml, error = %e, "Failed opening index.yaml within vendor folder, it might not have any devices");
                continue;
            }
        })?;

        for device_id in &devices.end_devices {
            let device_yml = vendor_dir.join(format!("{}.yaml", device_id));
            info!(path = ?device_yml, "Reading device file");
            let dev: Device = serde_yaml::from_reader(File::open(&device_yml)?)?;
            import_device(&vendor_dir, vendor, device_id, &dev).await?;
        }
    }

    Ok(())
}

async fn import_device(
    dir: &Path,
    vendor: &Vendor,
    device_id: &str,
    device: &Device,
) -> Result<()> {
    info!(vendor_id = %vendor.id, device_id = %device_id, "Importing device");
    let id_regex = regex::Regex::new(r"[^\w-]+").unwrap();

    for fw in &device.firmware_versions {
        for (region, profile) in &fw.profiles {
            info!(fw_version = %fw.version, region = %region, vendor_id = %profile.vendor_id, profile = %profile.id, codec = %profile.codec, "Found profile");
            let profile_yml = if profile.vendor_id.is_empty() {
                dir.join(format!("{}.yaml", profile.id))
            } else {
                dir.join("..")
                    .join(&profile.vendor_id)
                    .join(format!("{}.yaml", profile.id))
            };

            info!(path = ?profile_yml, "Reading profile");
            let prof: Profile = serde_yaml::from_reader(File::open(&profile_yml)?)?;

            let codec = if profile.codec.is_empty() {
                None
            } else {
                let codec_yml = dir.join(format!("{}.yaml", profile.codec));
                info!(path = ?codec_yml, "Reading codec");

                let codec: JsCodec = serde_yaml::from_reader(File::open(&codec_yml)?)?;
                let mut files_to_read: HashSet<String> = HashSet::new();

                if let Some(c) = &codec.uplink_decoder {
                    files_to_read.insert(c.filename.clone());
                }

                if let Some(c) = &codec.downlink_encoder {
                    files_to_read.insert(c.filename.clone());
                }

                if let Some(c) = &codec.downlink_decoder {
                    files_to_read.insert(c.filename.clone());
                }

                let mut codec_js: String = "".to_string();

                for f in &files_to_read {
                    let codec_f = dir.join(f);
                    info!(path = ?codec_f, "Reading codec function code");
                    codec_js.push_str(&read_to_string(codec_f)?);
                }

                Some(codec_js)
            };

            let regions: Vec<CommonName> = match region.as_ref() {
                "EU863-870" => vec![CommonName::EU868],
                "US902-928" => vec![CommonName::US915],
                "AU915-928" => vec![CommonName::AU915],
                "AS923" => vec![
                    CommonName::AS923,
                    CommonName::AS923_2,
                    CommonName::AS923_3,
                    CommonName::AS923_4,
                ],
                "CN779-787" => vec![CommonName::CN779],
                "EU433" => vec![CommonName::EU433],
                "CN470-510" => vec![CommonName::CN470],
                "KR920-923" => vec![CommonName::KR920],
                "IN865-867" => vec![CommonName::IN865],
                "RU864-870" => vec![CommonName::RU864],
                _ => {
                    return Err(anyhow!("Unexpected region: {}", region));
                }
            };

            for region in regions {
                let id = format!(
                    "{}-{}-{}-{}-{}",
                    vendor.id, device_id, fw.version, region, profile.id
                );
                let id = id_regex.replace_all(&id, "-").to_string();

                let dp = device_profile_template::DeviceProfileTemplate {
                    id,
                    name: truncate(&device.name, 100).to_string(),
                    description: format!(
                        "{}\n\nSource: https://github.com/TheThingsNetwork/lorawan-devices",
                        device.description
                    ),
                    vendor: vendor.name.clone(),
                    firmware: fw.version.clone(),
                    region,
                    mac_version: MacVersion::from_str(&prof.mac_version)?,
                    reg_params_revision: match prof.region_parameters_version.as_ref() {
                        "TS001-1.0" => Revision::A,
                        "TS001-1.0.1" => Revision::A,
                        "RP001-1.0.2" => Revision::A,
                        "RP001-1.0.2-RevB" => Revision::B,
                        "RP001-1.0.3-RevA" => Revision::A,
                        "RP001-1.1-RevA" => Revision::A,
                        "RP001-1.1-RevB" => Revision::B,
                        _ => Revision::from_str(&prof.region_parameters_version)?,
                    },
                    adr_algorithm_id: "default".into(),
                    payload_codec_runtime: match &codec {
                        None => Codec::NONE,
                        Some(_) => Codec::JS,
                    },
                    payload_codec_script: match &codec {
                        None => "".to_string(),
                        Some(v) => v.to_string(),
                    },
                    uplink_interval: 60 * 60,
                    device_status_req_interval: 1,
                    flush_queue_on_activate: true,
                    supports_otaa: prof.supports_join,
                    supports_class_b: prof.supports_class_b,
                    supports_class_c: prof.supports_class_c,
                    class_b_timeout: prof.class_b_timeout as i32,
                    class_b_ping_slot_nb_k: match prof.ping_slot_period {
                        128 => 7,
                        64 => 6,
                        32 => 5,
                        16 => 4,
                        8 => 3,
                        4 => 2,
                        2 => 1,
                        1 => 0,
                        _ => 0,
                    },
                    class_b_ping_slot_dr: prof.ping_slot_data_rate_index as i16,
                    class_b_ping_slot_freq: (prof.ping_slot_frequency * 1_000_000.0) as i64,
                    class_c_timeout: prof.class_c_timeout as i32,
                    abp_rx1_delay: prof.rx1_delay as i16,
                    abp_rx1_dr_offset: prof.rx1_data_rate_offset as i16,
                    abp_rx2_dr: prof.rx2_data_rate_index as i16,
                    abp_rx2_freq: (prof.rx2_frequency * 1_000_000.0) as i64,
                    ..Default::default()
                };

                device_profile_template::upsert(dp).await?;
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

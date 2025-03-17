/*
 * 模块概述
 * ========
 * 测试(Test)模块是ChirpStack LoRaWAN网络服务器的核心测试框架，负责提供集成测试环境和工具。
 * 该模块实现了一套完整的测试基础设施，包括测试环境准备、断言工具和各种LoRaWAN功能的测试用例。
 * 
 * 测试模块覆盖了ChirpStack的所有核心功能，包括设备激活(OTAA和ABP)、不同设备类别(Class A/B/C)
 * 的通信、多播组操作、中继功能等。通过这些测试，确保ChirpStack能够正确实现LoRaWAN协议规范，
 * 并在各种场景下可靠工作。
 * 
 * 该模块采用了模拟(Mock)技术来模拟网关和集成接口，使得测试可以在不依赖实际硬件的情况下运行，
 * 同时保持测试的真实性和完整性。
 *
 * 文件功能
 * ========
 * 本文件(mod.rs)是测试模块的入口点，提供了以下主要功能：
 * 1. 声明测试子模块，包括各种功能测试和断言工具
 * 2. 提供测试环境准备函数，设置数据库、Redis和配置
 * 3. 确保测试之间的隔离，防止并发测试相互干扰
 *
 * 主要组件
 * ========
 * - prepare(): 准备测试环境，设置配置、数据库和Redis
 * - TEST_MUX: 测试互斥锁，确保测试之间的隔离
 * - TRACING_INIT: 日志初始化控制，确保日志只初始化一次
 * - 各种测试子模块: 实现不同功能的测试用例
 *
 * 测试子模块
 * ==========
 * - assert: 提供断言工具，用于验证测试结果
 * - class_a_test: 测试Class A设备的上下行通信
 * - class_a_pr_test: 测试Class A设备的被动漫游
 * - class_b_test: 测试Class B设备的下行通信
 * - class_c_test: 测试Class C设备的下行通信
 * - multicast_test: 测试多播组功能
 * - otaa_test: 测试空中激活(OTAA)流程
 * - otaa_js_test: 测试与Join Server集成的OTAA流程
 * - otaa_pr_test: 测试被动漫游的OTAA流程
 * - relay_class_a_test: 测试中继设备的Class A通信
 * - relay_otaa_test: 测试中继设备的OTAA流程
 *
 * 关键流程
 * ========
 * 1. 测试环境准备流程:
 *    - 加载环境变量
 *    - 获取测试互斥锁，确保测试隔离
 *    - 初始化日志系统
 *    - 设置测试配置
 *    - 初始化存储层(PostgreSQL和Redis)
 *    - 重置数据库和Redis
 *    - 设置区域配置和ADR
 *
 * 注意事项
 * ========
 * - 测试隔离: 使用互斥锁确保测试之间不会相互干扰
 * - 资源清理: 每次测试前重置数据库和Redis，确保测试环境的干净
 * - 配置依赖: 测试需要正确配置PostgreSQL和Redis连接
 * - 模拟组件: 测试使用模拟的网关和集成接口，而不是实际硬件
 * - 并发控制: 测试框架设计为串行执行测试，避免并发问题
 */

use std::env;
use std::sync::{Mutex, Once};

use crate::{adr, config, region, storage};

mod assert;
mod class_a_pr_test;
mod class_a_test;
mod class_b_test;
mod class_c_test;
mod multicast_test;
mod otaa_js_test;
mod otaa_pr_test;
mod otaa_test;
mod relay_class_a_test;
mod relay_otaa_test;

static TRACING_INIT: Once = Once::new();

lazy_static! {
    static ref TEST_MUX: Mutex<()> = Mutex::new(());
}

pub async fn prepare<'a>() -> std::sync::MutexGuard<'a, ()> {
    dotenv::dotenv().ok();
    dotenv::from_filename(".env.local").ok();

    // Set a mutex lock to make sure database dependent tests are not overlapping. At the end of
    // the function the guard is returned, so that the mutex guard can be kept during the lifetime
    // of the function running the tests.
    let guard = TEST_MUX.lock().unwrap();

    // Set logger
    TRACING_INIT.call_once(|| {
        tracing_subscriber::fmt::init();
    });

    // set test config
    let mut conf: config::Configuration = Default::default();
    conf.postgresql.dsn = env::var("TEST_POSTGRESQL_DSN").unwrap();
    conf.redis.servers = vec![env::var("TEST_REDIS_URL").unwrap()];
    conf.sqlite.path = ":memory:".to_string();
    conf.network.enabled_regions = vec!["eu868".to_string()];
    conf.regions = vec![config::Region {
        id: "eu868".to_string(),
        description: "EU868".to_string(),
        common_name: lrwn::region::CommonName::EU868,
        user_info: "".into(),
        network: config::RegionNetwork {
            installation_margin: 10.0,
            rx1_delay: 1,
            rx2_frequency: 869525000,
            gateway_prefer_min_margin: 10.0,
            downlink_tx_power: -1,
            min_dr: 0,
            max_dr: 5,
            uplink_max_eirp: 16.0,
            class_b: config::ClassB {
                ping_slot_dr: 0,
                ping_slot_frequency: 868100000,
            },
            extra_channels: Vec::new(),
            enabled_uplink_channels: Vec::new(),
            ..Default::default()
        },
        gateway: config::RegionGateway {
            force_gws_private: false,
            channels: vec![],
            backend: config::GatewayBackend {
                enabled: "mqtt".into(),
                mqtt: config::GatewayBackendMqtt {
                    topic_prefix: "eu868".into(),
                    ..Default::default()
                },
            },
        },
    }];
    config::set(conf);

    // setup storage
    storage::setup().await.unwrap();

    // reset db
    storage::reset_db().await.unwrap();

    // flush redis db
    storage::reset_redis().await.unwrap();

    // setup region
    region::setup().unwrap();

    // setup adr
    adr::setup().await.unwrap();

    guard
}

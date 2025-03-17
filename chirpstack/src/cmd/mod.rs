/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的命令行工具集合，提供了各种管理和维护功能。
 * 在ChirpStack系统中，该模块负责提供命令行接口，使管理员能够执行各种系统操作。
 * 该模块与系统的其他部分（如存储、网关管理、设备管理等）进行交互，提供了一系列实用工具。
 *
 * 文件功能
 * ========
 * 该文件是cmd模块的入口点，负责导出所有可用的命令子模块。
 * 它定义了模块的结构，使其他部分可以访问各种命令功能。
 * 主要作用是组织和暴露各种命令行工具。
 *
 * 主要组件
 * ========
 * - configfile: 生成配置文件模板
 * - create_api_key: 创建API密钥
 * - import_legacy_lorawan_devices_repository: 导入旧版LoRaWAN设备库
 * - import_lorawan_device_profiles: 导入LoRaWAN设备配置文件
 * - migrate_ds_to_pg: 将设备会话从Redis迁移到PostgreSQL
 * - print_ds: 打印设备会话信息
 * - root: 启动ChirpStack服务器的主命令
 *
 * 关键流程
 * ========
 * 该文件本身不包含具体的业务逻辑，而是通过导出各个子模块来组织命令行功能。
 * 每个子模块实现特定的命令行功能，可以通过主程序的命令行接口调用。
 *
 * 注意事项
 * ========
 * - 添加新的命令行功能时，需要在此文件中添加相应的模块导出
 * - 所有命令模块应遵循统一的错误处理和日志记录模式
 * - 命令行工具应考虑安全性，特别是那些涉及API密钥创建和数据迁移的命令
 */

pub mod configfile;
pub mod create_api_key;
pub mod import_legacy_lorawan_devices_repository;
pub mod import_lorawan_device_profiles;
pub mod migrate_ds_to_pg;
pub mod print_ds;
pub mod root;

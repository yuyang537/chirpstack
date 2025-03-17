/*
 * 模块概述
 * ========
 * 流处理(Stream)模块是ChirpStack LoRaWAN网络服务器的核心组件之一，负责处理各种实时数据流，
 * 包括设备事件、帧日志、API请求和后端接口通信等。该模块提供了一套完整的流处理机制，使系统
 * 能够高效地处理、记录和分发大量的实时数据。
 * 
 * 流处理模块采用Redis流(Redis Streams)作为底层存储和消息队列机制，这使得ChirpStack能够
 * 以高吞吐量处理大量并发的数据流，同时保持数据的时序性和可靠性。通过这种方式，系统能够
 * 支持实时监控、事件追踪和历史数据分析等关键功能。
 * 
 * 该模块是ChirpStack与外部系统集成的重要桥梁，为应用层提供了丰富的数据接口，同时也为
 * 系统内部组件之间的通信提供了可靠的基础设施。
 *
 * 子模块功能
 * ==========
 * - api_request: 处理和记录API请求日志，用于监控和审计系统API使用情况
 * - backend_interfaces: 管理与LoRaWAN后端接口的通信日志，支持漫游和服务器间通信
 * - event: 处理设备和应用事件流，包括上行、下行、加入和状态事件等
 * - frame: 管理LoRaWAN帧日志，记录设备和网关的上行和下行通信
 * - meta: 处理元数据流，提供系统运行状态和性能指标的实时监控
 */

pub mod api_request;
pub mod backend_interfaces;
pub mod event;
pub mod frame;
pub mod meta;

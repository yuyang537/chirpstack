/*
 * 模块概述
 * ========
 * 中继(Relay)模块是ChirpStack LoRaWAN网络服务器的重要组件，负责实现LoRaWAN中继功能的管理和配置。
 * 在LoRaWAN网络架构中，中继设备允许扩展网络覆盖范围，使得远离网关的终端设备能够通过中继设备
 * 将数据转发到网关，从而解决覆盖范围有限的问题。
 * 
 * 该模块与设备管理和应用程序管理模块紧密集成，为ChirpStack提供了灵活的中继网络拓扑管理能力。
 * 它允许网络管理员配置哪些设备可以作为中继，以及哪些终端设备可以通过特定中继进行通信。
 * 
 * 在LoRaWAN协议中，中继功能是一项重要的网络扩展技术，特别适用于需要覆盖大面积或复杂环境
 * （如建筑物内部、地下或远程区域）的应用场景。本模块实现了LoRaWAN中继的配置和管理功能，
 * 使网络运营商能够灵活构建和优化网络拓扑。
 *
 * 文件功能
 * ========
 * 本文件(relay.rs)实现了中继管理的gRPC服务接口，提供了以下主要功能：
 * 1. 列出应用程序下的所有中继设备
 * 2. 向中继添加终端设备，建立中继关系
 * 3. 从中继移除终端设备，解除中继关系
 * 4. 列出通过特定中继通信的所有终端设备
 * 5. 验证用户对中继和终端设备的访问权限
 *
 * 主要组件
 * ========
 * - Relay: 核心服务结构体，实现了RelayService trait
 *   - new(): 创建Relay服务实例的构造函数
 *   - list(): 列出应用程序下的中继设备
 *   - add_device(): 向中继添加终端设备
 *   - remove_device(): 从中继移除终端设备
 *   - list_devices(): 列出通过中继通信的终端设备
 * - validator: 用于验证请求权限的组件
 * - test模块: 包含单元测试函数
 *
 * 关键流程
 * ========
 * 1. 中继设备列表查询流程:
 *    - 验证请求者对应用程序的访问权限
 *    - 根据应用程序ID筛选中继设备
 *    - 返回中继设备列表及其基本信息
 * 
 * 2. 中继关系建立流程:
 *    - 验证请求者对中继设备和终端设备的访问权限
 *    - 验证中继设备和终端设备的兼容性
 *    - 在数据库中创建中继与终端设备的关联关系
 *    - 更新设备的中继状态
 *
 * 3. 中继关系解除流程:
 *    - 验证请求者对中继设备的访问权限
 *    - 从数据库中删除中继与终端设备的关联关系
 *    - 更新设备的中继状态
 *
 * 注意事项
 * ========
 * - 安全性: 中继关系的建立和解除需要严格的访问控制，防止未授权的设备接入
 * - 兼容性: 中继设备和终端设备需要兼容的LoRaWAN版本和配置
 * - 网络拓扑: 中继关系会影响网络拓扑和数据流路径，需要谨慎规划
 * - 性能考虑: 中继会增加数据传输的延迟和复杂性，需要在覆盖范围和性能之间权衡
 * - 电池寿命: 作为中继的设备通常需要更多能源，应考虑其供电方式
 * - 协议限制: LoRaWAN中继功能有特定的协议限制和要求，需要遵循相关规范
 */

use std::str::FromStr;

use tonic::{Request, Response, Status};
use uuid::Uuid;

use chirpstack_api::api;
use chirpstack_api::api::relay_service_server::RelayService;
use lrwn::EUI64;

use super::auth::validator;
use super::error::ToStatus;
use super::helpers;

use crate::storage::relay;

pub struct Relay {
    validator: validator::RequestValidator,
}

impl Relay {
    pub fn new(validator: validator::RequestValidator) -> Self {
        Relay { validator }
    }
}

#[tonic::async_trait]
impl RelayService for Relay {
    async fn list(
        &self,
        request: Request<api::ListRelaysRequest>,
    ) -> Result<Response<api::ListRelaysResponse>, Status> {
        let req = request.get_ref();
        let app_id = Uuid::from_str(&req.application_id).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDevicesAccess::new(validator::Flag::List, app_id),
            )
            .await?;

        let filters = relay::RelayFilters {
            application_id: Some(app_id),
        };

        let count = relay::get_relay_count(&filters)
            .await
            .map_err(|e| e.status())?;
        let items = relay::list_relays(req.limit as i64, req.offset as i64, &filters)
            .await
            .map_err(|e| e.status())?;

        let mut resp = Response::new(api::ListRelaysResponse {
            total_count: count as u32,
            result: items
                .iter()
                .map(|r| api::RelayListItem {
                    dev_eui: r.dev_eui.to_string(),
                    name: r.name.clone(),
                })
                .collect(),
        });

        resp.metadata_mut()
            .insert("x-log-application_id", req.application_id.parse().unwrap());
        Ok(resp)
    }

    async fn add_device(
        &self,
        request: Request<api::AddRelayDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        let relay_dev_eui = EUI64::from_str(&req.relay_dev_eui).map_err(|e| e.status())?;
        let device_dev_eui = EUI64::from_str(&req.device_dev_eui).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDeviceAccess::new(validator::Flag::Update, relay_dev_eui),
            )
            .await?;
        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDeviceAccess::new(validator::Flag::Update, device_dev_eui),
            )
            .await?;

        relay::add_device(relay_dev_eui, device_dev_eui)
            .await
            .map_err(|e| e.status())?;

        let mut resp = Response::new(());
        resp.metadata_mut()
            .insert("x-log-relay_dev_eui", req.relay_dev_eui.parse().unwrap());
        resp.metadata_mut()
            .insert("x-log-device_dev_eui", req.device_dev_eui.parse().unwrap());

        Ok(resp)
    }

    async fn remove_device(
        &self,
        request: Request<api::RemoveRelayDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        let relay_dev_eui = EUI64::from_str(&req.relay_dev_eui).map_err(|e| e.status())?;
        let device_dev_eui = EUI64::from_str(&req.device_dev_eui).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDeviceAccess::new(validator::Flag::Update, relay_dev_eui),
            )
            .await?;
        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDeviceAccess::new(validator::Flag::Update, device_dev_eui),
            )
            .await?;

        relay::remove_device(relay_dev_eui, device_dev_eui)
            .await
            .map_err(|e| e.status())?;

        let mut resp = Response::new(());
        resp.metadata_mut()
            .insert("x-log-relay_dev_eui", req.relay_dev_eui.parse().unwrap());
        resp.metadata_mut()
            .insert("x-log-device_dev_eui", req.device_dev_eui.parse().unwrap());

        Ok(resp)
    }

    async fn list_devices(
        &self,
        request: Request<api::ListRelayDevicesRequest>,
    ) -> Result<Response<api::ListRelayDevicesResponse>, Status> {
        let req = request.get_ref();
        let relay_dev_eui = EUI64::from_str(&req.relay_dev_eui).map_err(|e| e.status())?;

        self.validator
            .validate(
                request.extensions(),
                validator::ValidateDeviceAccess::new(validator::Flag::Read, relay_dev_eui),
            )
            .await?;

        let filters = relay::DeviceFilters {
            relay_dev_eui: Some(relay_dev_eui),
        };

        let count = relay::get_device_count(&filters)
            .await
            .map_err(|e| e.status())?;
        let items = relay::list_devices(req.limit as i64, req.offset as i64, &filters)
            .await
            .map_err(|e| e.status())?;

        let mut resp = Response::new(api::ListRelayDevicesResponse {
            total_count: count as u32,
            result: items
                .iter()
                .map(|d| api::RelayDeviceListItem {
                    dev_eui: d.dev_eui.to_string(),
                    name: d.name.clone(),
                    created_at: Some(helpers::datetime_to_prost_timestamp(&d.created_at)),
                })
                .collect(),
        });

        resp.metadata_mut()
            .insert("x-log-relay_dev_eui", req.relay_dev_eui.parse().unwrap());
        Ok(resp)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::api::auth::validator::RequestValidator;
    use crate::api::auth::AuthID;
    use crate::storage::{application, device, device_profile, tenant, user};
    use crate::test;

    #[tokio::test]
    async fn test_relay() {
        let _guard = test::prepare().await;

        // setup admin user
        let u = user::User {
            is_admin: true,
            is_active: true,
            email: "admin@admin".into(),
            email_verified: true,
            ..Default::default()
        };
        let u = user::create(u).await.unwrap();

        // create tenant
        let t = tenant::create(tenant::Tenant {
            name: "test-tenant".into(),
            ..Default::default()
        })
        .await
        .unwrap();

        // create application
        let app = application::create(application::Application {
            name: "test-app".into(),
            tenant_id: t.id,
            ..Default::default()
        })
        .await
        .unwrap();

        // create device-profile
        let dp = device_profile::create(device_profile::DeviceProfile {
            name: "test-dp".into(),
            tenant_id: t.id,
            ..Default::default()
        })
        .await
        .unwrap();

        // create relay device-profile
        let dp_relay = device_profile::create(device_profile::DeviceProfile {
            name: "test-dp".into(),
            tenant_id: t.id,
            is_relay: true,
            ..Default::default()
        })
        .await
        .unwrap();

        // create devices
        let d_relay = device::create(device::Device {
            name: "relay-device".into(),
            dev_eui: EUI64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            device_profile_id: dp_relay.id,
            application_id: app.id,
            ..Default::default()
        })
        .await
        .unwrap();
        let d = device::create(device::Device {
            name: "device".into(),
            dev_eui: EUI64::from_be_bytes([2, 2, 3, 4, 5, 6, 7, 8]),
            device_profile_id: dp.id,
            application_id: app.id,
            ..Default::default()
        })
        .await
        .unwrap();

        // setup the api
        let service = Relay::new(RequestValidator::new());

        // list relays
        let list_req = get_request(
            &u.id,
            api::ListRelaysRequest {
                application_id: app.id.to_string(),
                limit: 10,
                ..Default::default()
            },
        );
        let list_resp = service.list(list_req).await.unwrap();
        assert_eq!(1, list_resp.get_ref().total_count);

        // add device
        let add_req = get_request(
            &u.id,
            api::AddRelayDeviceRequest {
                relay_dev_eui: d_relay.dev_eui.to_string(),
                device_dev_eui: d.dev_eui.to_string(),
            },
        );
        let _ = service.add_device(add_req).await.unwrap();

        // list devices
        let list_req = get_request(
            &u.id,
            api::ListRelayDevicesRequest {
                relay_dev_eui: d_relay.dev_eui.to_string(),
                limit: 10,
                ..Default::default()
            },
        );
        let list_resp = service.list_devices(list_req).await.unwrap();
        assert_eq!(1, list_resp.get_ref().total_count);
        assert_eq!(1, list_resp.get_ref().result.len());
        assert_eq!(d.dev_eui.to_string(), list_resp.get_ref().result[0].dev_eui);

        // remove device
        let remove_req = get_request(
            &u.id,
            api::RemoveRelayDeviceRequest {
                relay_dev_eui: d_relay.dev_eui.to_string(),
                device_dev_eui: d.dev_eui.to_string(),
            },
        );
        let _ = service.remove_device(remove_req).await.unwrap();
        let remove_req = get_request(
            &u.id,
            api::RemoveRelayDeviceRequest {
                relay_dev_eui: d_relay.dev_eui.to_string(),
                device_dev_eui: d.dev_eui.to_string(),
            },
        );
        assert!(service.remove_device(remove_req).await.is_err());
    }

    fn get_request<T>(user_id: &Uuid, req: T) -> Request<T> {
        let mut req = Request::new(req);
        req.extensions_mut().insert(AuthID::User(*user_id));
        req
    }
}

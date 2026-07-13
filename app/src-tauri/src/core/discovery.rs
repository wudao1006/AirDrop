use crate::{core::service, platform};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{collections::HashMap, net::IpAddr, thread};
use tauri::{AppHandle, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const SERVICE_TYPE: &str = "_localdrop._udp.local.";
pub(crate) const TRANSPORT_PORT: u16 = 43_721;

pub(crate) fn start(app: AppHandle) -> Result<(), String> {
    let state = app.state::<service::ServiceState>();
    let device_id = state.device_id().to_string();
    let device_name = state.device_name().to_string();
    drop(state);

    let daemon = ServiceDaemon::new().map_err(|error| format!("无法启动局域网发现：{error}"))?;
    let service_instance_id = Uuid::new_v4().simple().to_string();
    let instance_name = format!("AirDrop-{}", &service_instance_id[..12]);
    let hostname = format!("{}.local.", sanitize_hostname(&device_name));
    let properties = [
        ("protocol_version", "1"),
        ("service_instance_id", service_instance_id.as_str()),
        ("device_id", device_id.as_str()),
        ("device_name", device_name.as_str()),
        ("platform", platform::platform_name()),
        ("capabilities", "clipboard-slots,pairing"),
    ];
    let addresses: &[IpAddr] = &[];
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &hostname,
        addresses,
        TRANSPORT_PORT,
        &properties[..],
    )
    .map_err(|error| format!("无法创建局域网服务信息：{error}"))?
    .enable_addr_auto();
    daemon
        .register(info)
        .map_err(|error| format!("无法发布局域网服务：{error}"))?;
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|error| format!("无法浏览局域网设备：{error}"))?;

    thread::spawn(move || {
        let _daemon = daemon;
        let mut resolved_instances = HashMap::<String, String>::new();
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let Some(remote_device_id) = info.get_property_val_str("device_id") else {
                        continue;
                    };
                    if remote_device_id == device_id {
                        continue;
                    }
                    let Some(instance_id) = info.get_property_val_str("service_instance_id") else {
                        continue;
                    };
                    let Some(remote_name) = info.get_property_val_str("device_name") else {
                        continue;
                    };
                    let remote_platform =
                        info.get_property_val_str("platform").unwrap_or("unknown");
                    let protocol_version =
                        info.get_property_val_str("protocol_version").unwrap_or("0");
                    if protocol_version != "1" {
                        continue;
                    }
                    let addresses = info
                        .get_addresses()
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    resolved_instances
                        .insert(info.get_fullname().to_string(), instance_id.to_string());
                    let nearby = service::NearbyDevice {
                        instance_id: instance_id.to_string(),
                        device_id: remote_device_id.to_string(),
                        device_name: remote_name.to_string(),
                        platform: remote_platform.to_string(),
                        addresses,
                        port: info.get_port(),
                        last_seen_at: now(),
                        paired: false,
                    };
                    let state = app.state::<service::ServiceState>();
                    let _ = service::upsert_nearby_device(&state, &app, nearby.clone());
                    drop(state);
                    if app
                        .state::<service::ServiceState>()
                        .trusted_device(&nearby.device_id)
                        .ok()
                        .flatten()
                        .is_some()
                    {
                        app.state::<super::transport::TransportHandle>()
                            .connect_trusted(app.clone(), nearby);
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    if let Some(instance_id) = resolved_instances.remove(&fullname) {
                        let state = app.state::<service::ServiceState>();
                        let _ = service::remove_nearby_device(&state, &app, &instance_id);
                    }
                }
                _ => {}
            }
        }
        tracing::warn!("mDNS discovery loop stopped");
    });
    Ok(())
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn sanitize_hostname(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "airdrop-device".into()
    } else {
        trimmed.chars().take(48).collect()
    }
}

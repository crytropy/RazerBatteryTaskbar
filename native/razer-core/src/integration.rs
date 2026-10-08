use serde::Serialize;

use crate::frontend::{FrontendDeviceState, FrontendSnapshot};

pub const INTEGRATION_SCHEMA_VERSION: u32 = 1;
pub const INTEGRATION_SERVICE_NAME: &str = "RazerBatteryTaskbar";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: Option<String>,
    pub device_type: String,
    pub battery: Option<f32>,
    pub charging: Option<bool>,
    pub connected: bool,
}

impl From<&FrontendDeviceState> for IntegrationDevice {
    fn from(device: &FrontendDeviceState) -> Self {
        Self {
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            name: device.name.clone(),
            device_type: device.device_type.as_str().to_string(),
            battery: device.battery,
            charging: device.charging,
            connected: device.connected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSnapshot {
    pub schema_version: u32,
    pub service: &'static str,
    pub core_version: &'static str,
    pub transport_healthy: bool,
    pub consecutive_transport_failures: u32,
    pub primary_device: Option<IntegrationDevice>,
    pub devices: Vec<IntegrationDevice>,
}

impl IntegrationSnapshot {
    pub fn from_frontend(snapshot: &FrontendSnapshot) -> Self {
        Self {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            service: INTEGRATION_SERVICE_NAME,
            core_version: env!("CARGO_PKG_VERSION"),
            transport_healthy: snapshot.transport_healthy,
            consecutive_transport_failures: snapshot.consecutive_transport_failures,
            primary_device: snapshot
                .primary_device
                .as_ref()
                .map(IntegrationDevice::from),
            devices: snapshot
                .devices
                .iter()
                .map(IntegrationDevice::from)
                .collect(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{FrontendDeviceState, FrontendSnapshot};
    use crate::state::DeviceType;

    #[test]
    fn integration_json_is_versioned_and_privacy_safe() {
        let device = FrontendDeviceState {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(83.2),
            charging: None,
            connected: true,
        };

        let snapshot = FrontendSnapshot {
            devices: vec![device.clone()],
            primary_device: Some(device),
            transport_healthy: true,
            consecutive_transport_failures: 0,
        };

        let json = IntegrationSnapshot::from_frontend(&snapshot)
            .to_json()
            .unwrap();

        assert!(json.contains(r#""schemaVersion":1"#));
        assert!(json.contains(r#""service":"RazerBatteryTaskbar""#));
        assert!(json.contains(r#""deviceType":"mouse""#));
        assert!(json.contains(r#""productId":171"#));
        assert!(!json.contains("serial"));
        assert!(!json.contains("usb:1532"));
    }
}

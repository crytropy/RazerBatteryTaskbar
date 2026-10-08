use crate::state::{DeviceState, DeviceType};

#[derive(Debug, Clone, PartialEq)]
pub struct FrontendDeviceState {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: Option<String>,
    pub device_type: DeviceType,
    pub battery: Option<f32>,
    pub charging: Option<bool>,
    pub connected: bool,
}

impl From<&DeviceState> for FrontendDeviceState {
    fn from(device: &DeviceState) -> Self {
        Self {
            vendor_id: device.vendor_id,
            product_id: device.product_id,
            name: device.name.clone(),
            device_type: device.device_type,
            battery: device.battery,
            charging: device.charging,
            connected: device.connected,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrontendSnapshot {
    pub devices: Vec<FrontendDeviceState>,
    pub primary_device: Option<FrontendDeviceState>,
    pub transport_healthy: bool,
    pub consecutive_transport_failures: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DeviceReading, DeviceState};

    #[test]
    fn frontend_state_does_not_expose_serial_numbers_or_internal_ids() {
        let device = DeviceState::connected(DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            product_name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(83.2),
            charging: None,
            serial_number: Some("SECRET-SERIAL".to_string()),
        });

        let public = FrontendDeviceState::from(&device);

        assert_eq!(public.vendor_id, 0x1532);
        assert_eq!(public.product_id, 0x00AB);
        assert_eq!(public.battery, Some(83.2));

        let debug = format!("{public:?}");
        assert!(!debug.contains("SECRET-SERIAL"));
        assert!(!debug.contains("usb:1532"));
    }
}

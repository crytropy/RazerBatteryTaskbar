use crate::state::DeviceState;

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticsSnapshot {
    pub core_version: &'static str,
    pub transport_name: String,
    pub transport_ready: bool,
    pub consecutive_transport_failures: u32,
    pub last_transport_error: Option<String>,
    pub devices: Vec<DeviceState>,
    pub primary_device: Option<DeviceState>,
}

impl DiagnosticsSnapshot {
    pub fn to_privacy_safe_text(&self) -> String {
        let mut lines = vec![
            "RazerBattery core diagnostics".to_string(),
            format!("Core version: {}", self.core_version),
            format!("Transport: {}", self.transport_name),
            format!(
                "Transport ready: {}",
                if self.transport_ready { "yes" } else { "no" }
            ),
            format!(
                "Consecutive transport failures: {}",
                self.consecutive_transport_failures
            ),
            format!(
                "Last transport error: {}",
                self.last_transport_error.as_deref().unwrap_or("none")
            ),
            format!(
                "Primary device: {}",
                self.primary_device
                    .as_ref()
                    .and_then(|device| device.name.as_deref())
                    .unwrap_or("none")
            ),
            format!("Devices: {}", self.devices.len()),
        ];

        for device in &self.devices {
            let battery = device
                .battery
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| "unavailable".to_string());

            let charging = match device.charging {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            };

            lines.push(format!(
                "- {} [{:?}] VID:{:04X} PID:{:04X} connected={} battery={} charging={}",
                device.name.as_deref().unwrap_or("Unknown Razer device"),
                device.device_type,
                device.vendor_id,
                device.product_id,
                if device.connected { "yes" } else { "no" },
                battery,
                charging,
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DeviceReading, DeviceState, DeviceType};

    #[test]
    fn diagnostic_text_does_not_expose_serial_numbers_or_internal_ids() {
        let device = DeviceState::connected(DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            product_name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(83.2),
            charging: None,
            serial_number: Some("SECRET-SERIAL".to_string()),
        });

        let snapshot = DiagnosticsSnapshot {
            core_version: "0.1.0",
            transport_name: "fake".to_string(),
            transport_ready: true,
            consecutive_transport_failures: 0,
            last_transport_error: None,
            devices: vec![device.clone()],
            primary_device: Some(device),
        };

        let text = snapshot.to_privacy_safe_text();

        assert!(text.contains("Razer Basilisk V3 Pro Wireless"));
        assert!(text.contains("Transport ready: yes"));
        assert!(text.contains("VID:1532 PID:00AB"));
        assert!(text.contains("battery=83.2%"));
        assert!(!text.contains("SECRET-SERIAL"));
        assert!(!text.contains("usb:1532"));
    }
}

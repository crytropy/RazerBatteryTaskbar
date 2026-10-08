use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Mouse,
    Headset,
    Dock,
    Dongle,
    Unknown,
}

impl DeviceType {
    pub fn parse(value: &str) -> Self {
        match value {
            "mouse" => Self::Mouse,
            "headset" => Self::Headset,
            "dock" => Self::Dock,
            "dongle" => Self::Dongle,
            _ => Self::Unknown,
        }
    }

    pub fn priority(self) -> u8 {
        match self {
            Self::Mouse => 0,
            Self::Headset => 1,
            Self::Dock => 2,
            Self::Dongle => 3,
            Self::Unknown => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Headset => "headset",
            Self::Dock => "dock",
            Self::Dongle => "dongle",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceReading {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_name: Option<String>,
    pub device_type: DeviceType,
    pub battery: Option<f32>,
    pub charging: Option<bool>,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceState {
    pub id: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: Option<String>,
    pub device_type: DeviceType,
    pub battery: Option<f32>,
    pub charging: Option<bool>,
    pub connected: bool,
    pub serial_number: Option<String>,
    pub last_updated: SystemTime,
}

pub fn normalize_battery(value: Option<f32>) -> Option<f32> {
    value.and_then(|battery| {
        if battery.is_finite() {
            Some(battery.clamp(0.0, 100.0))
        } else {
            None
        }
    })
}

pub fn build_device_id(vendor_id: u16, product_id: u16, serial_number: Option<&str>) -> String {
    let base = format!("usb:{vendor_id:04X}:{product_id:04X}");

    match serial_number {
        Some(serial) if !serial.is_empty() => format!("{base}:{serial}"),
        _ => base,
    }
}

impl DeviceState {
    pub fn connected(reading: DeviceReading) -> Self {
        let id = build_device_id(
            reading.vendor_id,
            reading.product_id,
            reading.serial_number.as_deref(),
        );

        Self {
            id,
            vendor_id: reading.vendor_id,
            product_id: reading.product_id,
            name: reading.product_name,
            device_type: reading.device_type,
            battery: normalize_battery(reading.battery),
            charging: reading.charging,
            connected: true,
            serial_number: reading.serial_number,
            last_updated: SystemTime::now(),
        }
    }

    pub fn disconnected(previous: &Self) -> Self {
        Self {
            id: previous.id.clone(),
            vendor_id: previous.vendor_id,
            product_id: previous.product_id,
            name: previous.name.clone(),
            device_type: previous.device_type,
            battery: None,
            charging: None,
            connected: false,
            serial_number: previous.serial_number.clone(),
            last_updated: SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading() -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            product_name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(83.2),
            charging: None,
            serial_number: Some("ABC123".to_string()),
        }
    }

    #[test]
    fn normalizes_battery_values() {
        assert_eq!(normalize_battery(Some(-10.0)), Some(0.0));
        assert_eq!(normalize_battery(Some(42.5)), Some(42.5));
        assert_eq!(normalize_battery(Some(120.0)), Some(100.0));
        assert_eq!(normalize_battery(Some(f32::NAN)), None);
        assert_eq!(normalize_battery(None), None);
    }

    #[test]
    fn builds_the_same_device_id_shape_as_the_compatibility_core() {
        assert_eq!(
            build_device_id(0x1532, 0x00AB, Some("ABC123")),
            "usb:1532:00AB:ABC123",
        );
    }

    #[test]
    fn preserves_identity_when_a_device_disconnects() {
        let connected = DeviceState::connected(reading());
        let disconnected = DeviceState::disconnected(&connected);

        assert!(connected.connected);
        assert!(!disconnected.connected);
        assert_eq!(disconnected.id, connected.id);
        assert_eq!(disconnected.name, connected.name);
        assert_eq!(disconnected.battery, None);
    }
}

use crate::events::DeviceEvent;
use crate::frontend::FrontendDeviceState;
use crate::settings::NotificationSettings;
use crate::state::DeviceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    LowBattery,
    CriticalBattery,
}

impl NotificationKind {
    fn severity(self) -> u8 {
        match self {
            Self::LowBattery => 1,
            Self::CriticalBattery => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationRequest {
    pub kind: NotificationKind,
    pub device: FrontendDeviceState,
    pub title: String,
    pub body: String,
}

fn battery_notification_kind(
    battery: Option<f32>,
    settings: &NotificationSettings,
) -> Option<NotificationKind> {
    let battery = battery?;

    if battery <= settings.critical_battery_percent as f32 {
        return Some(NotificationKind::CriticalBattery);
    }

    if battery <= settings.low_battery_percent as f32 {
        return Some(NotificationKind::LowBattery);
    }

    None
}

fn create_request(kind: NotificationKind, device: &DeviceState) -> NotificationRequest {
    let name = device.name.as_deref().unwrap_or("Razer device");
    let battery = device.battery.unwrap_or_default();

    let (title, body) = match kind {
        NotificationKind::LowBattery => (
            "Razer battery low".to_string(),
            format!("{name} battery is {battery:.1}%."),
        ),
        NotificationKind::CriticalBattery => (
            "Razer battery critical".to_string(),
            format!("{name} battery is {battery:.1}%. Connect power or recharge soon."),
        ),
    };

    NotificationRequest {
        kind,
        device: FrontendDeviceState::from(device),
        title,
        body,
    }
}

pub fn evaluate_event(
    event: &DeviceEvent,
    settings: &NotificationSettings,
) -> Option<NotificationRequest> {
    if !settings.enabled {
        return None;
    }

    match event {
        DeviceEvent::Connected { current, .. } => {
            let kind = battery_notification_kind(current.battery, settings)?;
            Some(create_request(kind, current))
        }
        DeviceEvent::BatteryChanged { previous, current } => {
            let current_kind = battery_notification_kind(current.battery, settings)?;
            let previous_severity = battery_notification_kind(previous.battery, settings)
                .map(NotificationKind::severity)
                .unwrap_or(0);

            if current_kind.severity() <= previous_severity {
                return None;
            }

            Some(create_request(current_kind, current))
        }
        DeviceEvent::Disconnected { .. }
        | DeviceEvent::ChargingChanged { .. }
        | DeviceEvent::DevicesChanged { .. } => None,
    }
}

pub fn evaluate_events(
    events: &[DeviceEvent],
    settings: &NotificationSettings,
) -> Vec<NotificationRequest> {
    events
        .iter()
        .filter_map(|event| evaluate_event(event, settings))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DeviceReading, DeviceType};

    fn state(battery: Option<f32>) -> DeviceState {
        DeviceState::connected(DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            product_name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery,
            charging: None,
            serial_number: Some("SECRET-SERIAL".to_string()),
        })
    }

    #[test]
    fn connection_below_low_threshold_produces_a_low_battery_request() {
        let current = state(Some(15.0));
        let request = evaluate_event(
            &DeviceEvent::Connected {
                previous: None,
                current,
            },
            &NotificationSettings::default(),
        )
        .unwrap();

        assert_eq!(request.kind, NotificationKind::LowBattery);
        assert!(request.body.contains("15.0%"));
    }

    #[test]
    fn crossing_critical_threshold_produces_only_critical_notification() {
        let previous = state(Some(15.0));
        let current = state(Some(9.0));

        let request = evaluate_event(
            &DeviceEvent::BatteryChanged { previous, current },
            &NotificationSettings::default(),
        )
        .unwrap();

        assert_eq!(request.kind, NotificationKind::CriticalBattery);
    }

    #[test]
    fn remaining_inside_the_same_threshold_does_not_repeat_notifications() {
        let previous = state(Some(18.0));
        let current = state(Some(17.0));

        assert_eq!(
            evaluate_event(
                &DeviceEvent::BatteryChanged { previous, current },
                &NotificationSettings::default(),
            ),
            None
        );
    }

    #[test]
    fn battery_recovery_does_not_create_a_notification() {
        let previous = state(Some(8.0));
        let current = state(Some(50.0));

        assert_eq!(
            evaluate_event(
                &DeviceEvent::BatteryChanged { previous, current },
                &NotificationSettings::default(),
            ),
            None
        );
    }

    #[test]
    fn disabled_notifications_produce_no_requests() {
        let settings = NotificationSettings {
            enabled: false,
            ..NotificationSettings::default()
        };

        let current = state(Some(5.0));

        assert_eq!(
            evaluate_event(
                &DeviceEvent::Connected {
                    previous: None,
                    current,
                },
                &settings,
            ),
            None
        );
    }

    #[test]
    fn notification_payload_does_not_expose_serial_numbers() {
        let current = state(Some(5.0));
        let request = evaluate_event(
            &DeviceEvent::Connected {
                previous: None,
                current,
            },
            &NotificationSettings::default(),
        )
        .unwrap();

        let debug = format!("{request:?}");
        assert!(!debug.contains("SECRET-SERIAL"));
        assert!(!debug.contains("usb:1532"));
    }
}

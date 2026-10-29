//! "Primary device" submenu of the tray's right-click menu (#9).
//!
//! The entries follow the device list of the latest frontend snapshot, so the
//! submenu is rebuilt whenever the connected devices change. The choice is
//! keyed on the Razer product ID the core already uses for its primary
//! preference; serial numbers never reach this module.
use razer_core::device_db;
use razer_core::frontend::FrontendSnapshot;
use razer_core::settings::PrimaryDevicePreference;

/// One selectable device of the submenu, independent of the menu toolkit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice {
    pub product_id: u16,
    pub label: String,
    /// `false` for the preferred device while it is unplugged: it stays
    /// visible (and checked) so the user sees what the icon will return to.
    pub enabled: bool,
    pub checked: bool,
}

/// Device entries for the submenu: every connected device in the core's
/// order, plus the preferred device when it is currently offline.
pub fn device_choices(
    snapshot: Option<&FrontendSnapshot>,
    preference: &PrimaryDevicePreference,
) -> Vec<DeviceChoice> {
    let devices = snapshot
        .map(|snapshot| snapshot.devices.as_slice())
        .unwrap_or(&[]);
    let preferred = match preference {
        PrimaryDevicePreference::Auto => None,
        PrimaryDevicePreference::ProductId(product_id) => Some(*product_id),
    };

    let mut choices: Vec<DeviceChoice> = devices
        .iter()
        .filter(|device| device.connected)
        .map(|device| DeviceChoice {
            product_id: device.product_id,
            label: device
                .name
                .clone()
                .unwrap_or_else(|| fallback_label(device.product_id)),
            enabled: true,
            checked: preferred == Some(device.product_id),
        })
        .collect();

    if let Some(product_id) = preferred
        && !choices.iter().any(|choice| choice.product_id == product_id)
    {
        // Seen earlier this session, else the product database, else the ID.
        let name = devices
            .iter()
            .find(|device| device.product_id == product_id)
            .and_then(|device| device.name.clone())
            .or_else(|| device_db::get_product(product_id).map(|product| product.name.clone()))
            .unwrap_or_else(|| fallback_label(product_id));
        choices.push(DeviceChoice {
            product_id,
            label: format!("{name} (disconnected)"),
            enabled: false,
            checked: true,
        });
    }

    choices
}

fn fallback_label(product_id: u16) -> String {
    format!("Razer device {product_id:04X}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::frontend::{FrontendDeviceState, PollStatistics};
    use razer_core::state::DeviceType;

    fn device(
        product_id: u16,
        name: &str,
        device_type: DeviceType,
        connected: bool,
    ) -> FrontendDeviceState {
        FrontendDeviceState {
            vendor_id: 0x1532,
            product_id,
            name: Some(name.to_string()),
            device_type,
            battery: connected.then_some(50.0),
            charging: None,
            connected,
        }
    }

    fn snapshot(devices: Vec<FrontendDeviceState>) -> FrontendSnapshot {
        let primary_device = devices.iter().find(|device| device.connected).cloned();
        FrontendSnapshot {
            devices,
            primary_device,
            transport_name: "windows-hid".to_string(),
            transport_ready: true,
            transport_healthy: true,
            consecutive_transport_failures: 0,
            transport_error_kind: None,
            poll_statistics: PollStatistics::default(),
        }
    }

    fn mouse(connected: bool) -> FrontendDeviceState {
        device(
            0x00B7,
            "Razer Deathadder V3 Pro Wireless",
            DeviceType::Mouse,
            connected,
        )
    }

    fn headset(connected: bool) -> FrontendDeviceState {
        device(
            0x0527,
            "Razer BlackShark V2 Pro",
            DeviceType::Headset,
            connected,
        )
    }

    #[test]
    fn connected_devices_are_listed_in_core_order_and_the_preferred_one_is_checked() {
        let snapshot = snapshot(vec![mouse(true), headset(true)]);

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::Auto);
        assert_eq!(
            choices,
            vec![
                DeviceChoice {
                    product_id: 0x00B7,
                    label: "Razer Deathadder V3 Pro Wireless".to_string(),
                    enabled: true,
                    checked: false,
                },
                DeviceChoice {
                    product_id: 0x0527,
                    label: "Razer BlackShark V2 Pro".to_string(),
                    enabled: true,
                    checked: false,
                },
            ]
        );

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::ProductId(0x0527));
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.checked)
                .collect::<Vec<_>>(),
            vec![false, true]
        );
    }

    #[test]
    fn a_single_connected_device_is_still_one_selectable_entry() {
        let snapshot = snapshot(vec![mouse(true)]);

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::ProductId(0x00B7));
        assert_eq!(choices.len(), 1);
        assert!(choices[0].enabled);
        assert!(choices[0].checked);
    }

    #[test]
    fn no_snapshot_or_no_devices_yields_no_device_entries() {
        assert!(device_choices(None, &PrimaryDevicePreference::Auto).is_empty());
        assert!(
            device_choices(Some(&snapshot(Vec::new())), &PrimaryDevicePreference::Auto).is_empty()
        );
    }

    #[test]
    fn unplugged_preferred_device_stays_visible_but_disabled_and_checked() {
        let snapshot = snapshot(vec![mouse(true), headset(false)]);

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::ProductId(0x0527));
        assert_eq!(
            choices,
            vec![
                DeviceChoice {
                    product_id: 0x00B7,
                    label: "Razer Deathadder V3 Pro Wireless".to_string(),
                    enabled: true,
                    checked: false,
                },
                DeviceChoice {
                    product_id: 0x0527,
                    label: "Razer BlackShark V2 Pro (disconnected)".to_string(),
                    enabled: false,
                    checked: true,
                },
            ]
        );
    }

    #[test]
    fn preferred_device_never_seen_this_session_is_named_from_the_product_database() {
        // settings.json from an earlier session prefers the wired twin that is not plugged in.
        let snapshot = snapshot(vec![mouse(true)]);

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::ProductId(0x00B6));
        assert_eq!(choices.len(), 2);
        assert_eq!(
            choices[1].label,
            "Razer Deathadder V3 Pro Wired (disconnected)"
        );
        assert!(!choices[1].enabled);
        assert!(choices[1].checked);

        // An unknown product ID still gets a stable label instead of an error.
        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::ProductId(0x7777));
        assert_eq!(choices[1].label, "Razer device 7777 (disconnected)");
    }
}

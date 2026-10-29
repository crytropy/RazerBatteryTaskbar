//! "Primary device" submenu of the tray's right-click menu (#9).
//!
//! The entries follow the device list of the latest frontend snapshot, so the
//! submenu is rebuilt whenever the connected devices change. The choice is
//! keyed on the Razer product ID the core already uses for its primary
//! preference; serial numbers never reach this module.
use std::error::Error;

use razer_core::device_db;
use razer_core::frontend::FrontendSnapshot;
use razer_core::settings::PrimaryDevicePreference;
use tray_icon::menu::{CheckMenuItem, MenuEvent, Submenu};

const AUTOMATIC_ID: &str = "primary-device.auto";

/// The submenu itself: a stable "Automatic" entry followed by one check item
/// per [`DeviceChoice`]. The device items are torn down and re-appended when
/// the choices change; `tray-icon`/`muda` support that on a live submenu.
pub struct PrimaryDeviceMenu {
    pub root: Submenu,
    automatic_item: CheckMenuItem,
    device_items: Vec<(u16, CheckMenuItem)>,
    choices: Vec<DeviceChoice>,
}

impl PrimaryDeviceMenu {
    pub fn new(preference: &PrimaryDevicePreference) -> Result<Self, Box<dyn Error>> {
        let root = Submenu::with_id("primary-device", "Primary device", true);
        let automatic_item = CheckMenuItem::with_id(
            AUTOMATIC_ID,
            "Automatic",
            true,
            *preference == PrimaryDevicePreference::Auto,
            None,
        );
        root.append(&automatic_item)?;

        Ok(Self {
            root,
            automatic_item,
            device_items: Vec::new(),
            choices: Vec::new(),
        })
    }

    /// Bring the submenu in line with the latest snapshot and preference.
    /// Returns `true` when the device entries were rebuilt.
    pub fn update(
        &mut self,
        snapshot: Option<&FrontendSnapshot>,
        preference: &PrimaryDevicePreference,
    ) -> bool {
        self.automatic_item
            .set_checked(*preference == PrimaryDevicePreference::Auto);

        let choices = device_choices(snapshot, preference);
        if choices == self.choices {
            return false;
        }

        for (_, item) in self.device_items.drain(..) {
            if let Err(error) = self.root.remove(&item) {
                eprintln!("failed to remove a primary-device entry: {error}");
            }
        }

        for choice in &choices {
            let item = CheckMenuItem::with_id(
                format!("primary-device.{:04x}", choice.product_id),
                &choice.label,
                choice.enabled,
                choice.checked,
                None,
            );
            if let Err(error) = self.root.append(&item) {
                eprintln!("failed to add a primary-device entry: {error}");
                continue;
            }
            self.device_items.push((choice.product_id, item));
        }

        self.choices = choices;
        true
    }

    /// The preference a click on this submenu stands for, if the event is ours.
    pub fn preference_for_event(&self, event: &MenuEvent) -> Option<PrimaryDevicePreference> {
        if event.id == *self.automatic_item.id() {
            return Some(PrimaryDevicePreference::Auto);
        }

        self.device_items.iter().find_map(|(product_id, item)| {
            (event.id == *item.id()).then_some(PrimaryDevicePreference::ProductId(*product_id))
        })
    }
}

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
    use tray_icon::menu::{MenuId, MenuItemKind};

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

    fn checked_labels(menu: &PrimaryDeviceMenu) -> Vec<(String, bool, bool)> {
        menu.root
            .items()
            .iter()
            .map(|item| match item {
                MenuItemKind::Check(item) => (item.text(), item.is_enabled(), item.is_checked()),
                other => panic!("unexpected submenu child: {:?}", other.id()),
            })
            .collect()
    }

    fn event_for(menu: &PrimaryDeviceMenu, position: usize) -> MenuEvent {
        MenuEvent {
            id: menu.root.items()[position].id().clone(),
        }
    }

    #[test]
    fn submenu_is_rebuilt_from_each_snapshot_and_maps_clicks_back_to_preferences() {
        let mut menu = PrimaryDeviceMenu::new(&PrimaryDevicePreference::Auto).unwrap();
        assert_eq!(
            checked_labels(&menu),
            vec![("Automatic".to_string(), true, true)],
            "before the first snapshot only the stable entry exists"
        );

        assert!(menu.update(
            Some(&snapshot(vec![mouse(true), headset(true)])),
            &PrimaryDevicePreference::Auto
        ));
        assert_eq!(
            checked_labels(&menu),
            vec![
                ("Automatic".to_string(), true, true),
                ("Razer Deathadder V3 Pro Wireless".to_string(), true, false),
                ("Razer BlackShark V2 Pro".to_string(), true, false),
            ]
        );
        assert!(
            !menu.update(
                Some(&snapshot(vec![mouse(true), headset(true)])),
                &PrimaryDevicePreference::Auto
            ),
            "an unchanged device list does not rebuild the submenu"
        );

        assert_eq!(
            menu.preference_for_event(&event_for(&menu, 2)),
            Some(PrimaryDevicePreference::ProductId(0x0527))
        );
        assert_eq!(
            menu.preference_for_event(&event_for(&menu, 0)),
            Some(PrimaryDevicePreference::Auto)
        );
        assert_eq!(
            menu.preference_for_event(&MenuEvent {
                id: MenuId::new("settings")
            }),
            None
        );

        // The headset is unplugged while it is the preference.
        assert!(menu.update(
            Some(&snapshot(vec![mouse(true), headset(false)])),
            &PrimaryDevicePreference::ProductId(0x0527)
        ));
        assert_eq!(
            checked_labels(&menu),
            vec![
                ("Automatic".to_string(), true, false),
                ("Razer Deathadder V3 Pro Wireless".to_string(), true, false),
                (
                    "Razer BlackShark V2 Pro (disconnected)".to_string(),
                    false,
                    true
                ),
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

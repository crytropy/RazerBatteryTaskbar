//! "Primary device" submenu of the tray's right-click menu (#9).
//!
//! The entries follow the device list of the latest frontend snapshot, so the
//! submenu is rebuilt whenever the connected devices change. The choice is
//! keyed on the Razer product ID the core already uses for its primary
//! preference; serial numbers never reach this module.
use std::collections::BTreeMap;
use std::error::Error;

use razer_core::device_db;
use razer_core::frontend::FrontendSnapshot;
use razer_core::settings::PrimaryDevicePreference;
use tray_icon::menu::{CheckMenuItem, MenuEvent, Submenu};

const AUTOMATIC_ID: &str = "primary-device.auto";

/// Device names seen in earlier snapshots of this session, by product ID.
/// They label a preferred device that has since left the device list without
/// reaching past the snapshot into the product database.
pub type KnownNames = BTreeMap<u16, String>;

/// Resolve a saved preference to the canonical product ID the readings carry:
/// a wired / wireless twin (`00B6` on the cable) maps to its logical device
/// (`00B7`), everything else is unchanged.
pub fn canonical_preference(preference: &PrimaryDevicePreference) -> PrimaryDevicePreference {
    match preference {
        PrimaryDevicePreference::Auto => PrimaryDevicePreference::Auto,
        PrimaryDevicePreference::ProductId(product_id) => PrimaryDevicePreference::ProductId(
            device_db::logical_device(*product_id)
                .map(|device| device.product_id)
                .unwrap_or(*product_id),
        ),
    }
}

/// The submenu itself: a stable "Automatic" entry followed by one check item
/// per [`DeviceChoice`]. The device items are torn down and re-appended when
/// the choices change; `tray-icon`/`muda` support that on a live submenu.
pub struct PrimaryDeviceMenu {
    pub root: Submenu,
    automatic_item: CheckMenuItem,
    device_items: Vec<(u16, CheckMenuItem)>,
    choices: Vec<DeviceChoice>,
    known_names: KnownNames,
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
            known_names: KnownNames::new(),
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

        if let Some(snapshot) = snapshot {
            for device in &snapshot.devices {
                if let Some(name) = &device.name {
                    self.known_names.insert(device.product_id, name.clone());
                }
            }
        }

        let choices = device_choices(snapshot, preference, &self.known_names);
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
    /// Device name, with ` (current)` appended when the icon shows this
    /// device although another choice is checked (`Automatic`, or a preferred
    /// device that is unplugged), and ` (disconnected)` for that unplugged
    /// preferred device.
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
    known_names: &KnownNames,
) -> Vec<DeviceChoice> {
    let devices = snapshot
        .map(|snapshot| snapshot.devices.as_slice())
        .unwrap_or(&[]);
    let preferred = match canonical_preference(preference) {
        PrimaryDevicePreference::Auto => None,
        PrimaryDevicePreference::ProductId(product_id) => Some(product_id),
    };
    let current = snapshot
        .and_then(|snapshot| snapshot.primary_device.as_ref())
        .filter(|device| device.connected)
        .map(|device| device.product_id);

    let mut choices: Vec<DeviceChoice> = devices
        .iter()
        .filter(|device| device.connected)
        .map(|device| {
            let checked = preferred == Some(device.product_id);
            let name = device
                .name
                .clone()
                .unwrap_or_else(|| fallback_label(device.product_id));
            DeviceChoice {
                product_id: device.product_id,
                label: if !checked && current == Some(device.product_id) {
                    format!("{name} (current)")
                } else {
                    name
                },
                enabled: true,
                checked,
            }
        })
        .collect();

    if let Some(product_id) = preferred
        && !choices.iter().any(|choice| choice.product_id == product_id)
    {
        // Named from this snapshot, else an earlier one, else the ID.
        let name = devices
            .iter()
            .find(|device| device.product_id == product_id)
            .and_then(|device| device.name.clone())
            .or_else(|| known_names.get(&product_id).cloned())
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

    /// A snapshot the way the core builds it: the preferred device is primary
    /// while it is connected, otherwise the first connected device.
    fn snapshot_for(
        devices: Vec<FrontendDeviceState>,
        preference: &PrimaryDevicePreference,
    ) -> FrontendSnapshot {
        let preferred = match preference {
            PrimaryDevicePreference::Auto => None,
            PrimaryDevicePreference::ProductId(product_id) => Some(*product_id),
        };
        let primary_device = devices
            .iter()
            .find(|device| device.connected && Some(device.product_id) == preferred)
            .or_else(|| devices.iter().find(|device| device.connected))
            .cloned();
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

    fn snapshot(devices: Vec<FrontendDeviceState>) -> FrontendSnapshot {
        snapshot_for(devices, &PrimaryDevicePreference::Auto)
    }

    fn no_names() -> KnownNames {
        KnownNames::new()
    }

    fn mouse(connected: bool) -> FrontendDeviceState {
        device(
            0x00B7,
            "Razer DeathAdder V3 Pro",
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
        let preference = PrimaryDevicePreference::ProductId(0x0527);
        let snapshot = snapshot_for(vec![mouse(true), headset(true)], &preference);

        let choices = device_choices(Some(&snapshot), &preference, &no_names());
        assert_eq!(
            choices,
            vec![
                DeviceChoice {
                    product_id: 0x00B7,
                    label: "Razer DeathAdder V3 Pro".to_string(),
                    enabled: true,
                    checked: false,
                },
                DeviceChoice {
                    product_id: 0x0527,
                    label: "Razer BlackShark V2 Pro".to_string(),
                    enabled: true,
                    checked: true,
                },
            ]
        );
    }

    /// Spec #9: under `Automatic` the device the icon actually shows is marked
    /// as current, while `Automatic` itself stays the checked choice.
    #[test]
    fn automatic_marks_the_device_the_icon_currently_shows() {
        let snapshot = snapshot(vec![mouse(true), headset(true)]);

        let choices = device_choices(Some(&snapshot), &PrimaryDevicePreference::Auto, &no_names());
        assert_eq!(
            choices,
            vec![
                DeviceChoice {
                    product_id: 0x00B7,
                    label: "Razer DeathAdder V3 Pro (current)".to_string(),
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
    }

    #[test]
    fn a_single_connected_device_is_still_one_selectable_entry() {
        let preference = PrimaryDevicePreference::ProductId(0x00B7);
        let snapshot = snapshot_for(vec![mouse(true)], &preference);

        let choices = device_choices(Some(&snapshot), &preference, &no_names());
        assert_eq!(choices.len(), 1);
        assert!(choices[0].enabled);
        assert!(choices[0].checked);
        assert_eq!(choices[0].label, "Razer DeathAdder V3 Pro");
    }

    #[test]
    fn no_snapshot_or_no_devices_yields_no_device_entries() {
        assert!(device_choices(None, &PrimaryDevicePreference::Auto, &no_names()).is_empty());
        assert!(
            device_choices(
                Some(&snapshot(Vec::new())),
                &PrimaryDevicePreference::Auto,
                &no_names()
            )
            .is_empty()
        );
    }

    #[test]
    fn unplugged_preferred_device_stays_visible_but_disabled_and_checked() {
        let preference = PrimaryDevicePreference::ProductId(0x0527);
        let snapshot = snapshot_for(vec![mouse(true), headset(false)], &preference);

        // The icon has fallen back to the mouse, so the mouse is the current
        // device while the unplugged headset stays the checked preference.
        let choices = device_choices(Some(&snapshot), &preference, &no_names());
        assert_eq!(
            choices,
            vec![
                DeviceChoice {
                    product_id: 0x00B7,
                    label: "Razer DeathAdder V3 Pro (current)".to_string(),
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

    /// Review #3: a `settings.json` written while the mouse was on its cable
    /// prefers the wired twin `00B6`; readings always carry the canonical
    /// `00B7`, so the preference must resolve to the same entry.
    #[test]
    fn wired_twin_preference_resolves_to_the_canonical_device() {
        assert_eq!(
            canonical_preference(&PrimaryDevicePreference::ProductId(0x00B6)),
            PrimaryDevicePreference::ProductId(0x00B7)
        );
        assert_eq!(
            canonical_preference(&PrimaryDevicePreference::ProductId(0x00B7)),
            PrimaryDevicePreference::ProductId(0x00B7)
        );
        assert_eq!(
            canonical_preference(&PrimaryDevicePreference::ProductId(0x7777)),
            PrimaryDevicePreference::ProductId(0x7777)
        );
        assert_eq!(
            canonical_preference(&PrimaryDevicePreference::Auto),
            PrimaryDevicePreference::Auto
        );

        let preference = PrimaryDevicePreference::ProductId(0x00B6);
        let snapshot = snapshot_for(
            vec![mouse(true)],
            &PrimaryDevicePreference::ProductId(0x00B7),
        );
        let choices = device_choices(Some(&snapshot), &preference, &no_names());
        assert_eq!(
            choices,
            vec![DeviceChoice {
                product_id: 0x00B7,
                label: "Razer DeathAdder V3 Pro".to_string(),
                enabled: true,
                checked: true,
            }],
            "no disconnected ghost entry for the wired twin"
        );
    }

    #[test]
    fn preferred_device_never_seen_this_session_gets_a_stable_label() {
        let snapshot = snapshot(vec![mouse(true)]);

        // Remembered from an earlier snapshot of this session.
        let mut names = no_names();
        names.insert(0x0527, "Razer BlackShark V2 Pro".to_string());
        let choices = device_choices(
            Some(&snapshot),
            &PrimaryDevicePreference::ProductId(0x0527),
            &names,
        );
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[1].label, "Razer BlackShark V2 Pro (disconnected)");
        assert!(!choices[1].enabled);
        assert!(choices[1].checked);

        // Never seen: a stable label from the product ID, no database lookup.
        let choices = device_choices(
            Some(&snapshot),
            &PrimaryDevicePreference::ProductId(0x7777),
            &no_names(),
        );
        assert_eq!(choices[1].label, "Razer device 7777 (disconnected)");
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
                ("Razer DeathAdder V3 Pro (current)".to_string(), true, false),
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
        let preference = PrimaryDevicePreference::ProductId(0x0527);
        assert!(menu.update(
            Some(&snapshot_for(
                vec![mouse(true), headset(false)],
                &preference
            )),
            &preference
        ));
        assert_eq!(
            checked_labels(&menu),
            vec![
                ("Automatic".to_string(), true, false),
                ("Razer DeathAdder V3 Pro (current)".to_string(), true, false),
                (
                    "Razer BlackShark V2 Pro (disconnected)".to_string(),
                    false,
                    true
                ),
            ]
        );

        // Review #12: the headset drops out of the device list entirely, but
        // the menu still remembers its name from the earlier snapshot, so the
        // entries (and therefore the submenu) stay the same.
        assert!(!menu.update(
            Some(&snapshot_for(vec![mouse(true)], &preference)),
            &preference
        ));
        assert_eq!(
            checked_labels(&menu)[2],
            (
                "Razer BlackShark V2 Pro (disconnected)".to_string(),
                false,
                true
            )
        );
    }
}

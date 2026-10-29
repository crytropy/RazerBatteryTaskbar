use std::error::Error;

use razer_core::settings::AppSettings;
use tray_icon::menu::{CheckMenuItem, MenuEvent, MenuItem, Submenu};

/// The native tray polls at a fixed interval; there is no menu for it. One
/// poll opens the HID handle once and spends about 1.2 s on its two queries
/// (ADR-0001), so 10 s keeps the handle idle most of the time while still
/// letting the icon follow the charging cable within seconds: the cable raises
/// no USB event while the receiver is connected (issue #22), so the poll is
/// the only thing that notices it. Issue #24 removed the user-facing option.
pub const POLL_INTERVAL_SECONDS: u64 = 10;

const LOW_BATTERY_OPTIONS: &[(u8, &str)] = &[
    (10, "10%"),
    (15, "15%"),
    (20, "20%"),
    (25, "25%"),
    (30, "30%"),
];

const CRITICAL_BATTERY_OPTIONS: &[(u8, &str)] = &[(5, "5%"), (10, "10%"), (15, "15%"), (20, "20%")];

pub struct SettingsMenu {
    pub root: Submenu,
    notifications_item: CheckMenuItem,
    test_notification_item: MenuItem,
    start_with_windows_item: CheckMenuItem,
    low_battery_items: Vec<(u8, CheckMenuItem)>,
    critical_battery_items: Vec<(u8, CheckMenuItem)>,
}

impl SettingsMenu {
    pub fn new(settings: &AppSettings) -> Result<Self, Box<dyn Error>> {
        let root = Submenu::with_id("settings", "Settings", true);

        let notifications_item = CheckMenuItem::with_id(
            "settings.notifications",
            "Notifications",
            true,
            settings.notifications.enabled,
            None,
        );
        root.append(&notifications_item)?;

        let test_notification_item = MenuItem::with_id(
            "settings.test-notification",
            "Send test notification",
            true,
            None,
        );
        root.append(&test_notification_item)?;

        let low_battery_menu =
            Submenu::with_id("settings.low-battery", "Low battery threshold", true);
        let low_battery_items = LOW_BATTERY_OPTIONS
            .iter()
            .map(|(percent, label)| {
                let item = CheckMenuItem::with_id(
                    format!("settings.low-battery.{percent}"),
                    label,
                    true,
                    settings.notifications.low_battery_percent == *percent,
                    None,
                );
                low_battery_menu.append(&item)?;
                Ok((*percent, item))
            })
            .collect::<Result<Vec<_>, tray_icon::menu::Error>>()?;
        root.append(&low_battery_menu)?;

        let critical_battery_menu = Submenu::with_id(
            "settings.critical-battery",
            "Critical battery threshold",
            true,
        );
        let critical_battery_items = CRITICAL_BATTERY_OPTIONS
            .iter()
            .map(|(percent, label)| {
                let item = CheckMenuItem::with_id(
                    format!("settings.critical-battery.{percent}"),
                    label,
                    *percent <= settings.notifications.low_battery_percent,
                    settings.notifications.critical_battery_percent == *percent,
                    None,
                );
                critical_battery_menu.append(&item)?;
                Ok((*percent, item))
            })
            .collect::<Result<Vec<_>, tray_icon::menu::Error>>()?;
        root.append(&critical_battery_menu)?;

        let start_with_windows_item = CheckMenuItem::with_id(
            "settings.start-with-windows",
            "Start with Windows",
            true,
            settings.start_with_windows,
            None,
        );
        root.append(&start_with_windows_item)?;

        let menu = Self {
            root,
            notifications_item,
            test_notification_item,
            start_with_windows_item,
            low_battery_items,
            critical_battery_items,
        };

        menu.sync(settings);
        Ok(menu)
    }

    pub fn is_notifications_event(&self, event: &MenuEvent) -> bool {
        event.id == *self.notifications_item.id()
    }

    pub fn is_test_notification_event(&self, event: &MenuEvent) -> bool {
        event.id == *self.test_notification_item.id()
    }

    pub fn is_start_with_windows_event(&self, event: &MenuEvent) -> bool {
        event.id == *self.start_with_windows_item.id()
    }

    pub fn low_battery_for_event(&self, event: &MenuEvent) -> Option<u8> {
        self.low_battery_items
            .iter()
            .find_map(|(percent, item)| (event.id == *item.id()).then_some(*percent))
    }

    pub fn critical_battery_for_event(&self, event: &MenuEvent) -> Option<u8> {
        self.critical_battery_items
            .iter()
            .find_map(|(percent, item)| (event.id == *item.id()).then_some(*percent))
    }

    pub fn sync(&self, settings: &AppSettings) {
        self.notifications_item
            .set_checked(settings.notifications.enabled);
        self.start_with_windows_item
            .set_checked(settings.start_with_windows);

        for (percent, item) in &self.low_battery_items {
            item.set_checked(settings.notifications.low_battery_percent == *percent);
        }

        for (percent, item) in &self.critical_battery_items {
            item.set_enabled(*percent <= settings.notifications.low_battery_percent);
            item.set_checked(settings.notifications.critical_battery_percent == *percent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_poll_interval_is_a_valid_core_setting_and_the_core_default() {
        assert_eq!(POLL_INTERVAL_SECONDS, 10);
        let settings = AppSettings {
            poll_interval_seconds: POLL_INTERVAL_SECONDS,
            ..AppSettings::default()
        };
        settings.validate().unwrap();
        assert_eq!(
            AppSettings::default().poll_interval_seconds,
            POLL_INTERVAL_SECONDS
        );
    }

    #[test]
    fn settings_menu_has_no_poll_interval_entry() {
        let menu = SettingsMenu::new(&AppSettings::default()).unwrap();
        let ids: Vec<String> = menu
            .root
            .items()
            .iter()
            .map(|item| item.id().0.clone())
            .collect();
        assert!(!ids.is_empty());
        assert!(
            ids.iter().all(|id| !id.contains("poll-interval")),
            "poll interval must not be user-configurable: {ids:?}"
        );
    }

    #[test]
    fn all_threshold_combinations_exposed_by_the_menu_can_be_valid() {
        for (low, _) in LOW_BATTERY_OPTIONS {
            for (critical, _) in CRITICAL_BATTERY_OPTIONS {
                if critical > low {
                    continue;
                }

                let mut settings = AppSettings::default();
                settings.notifications.low_battery_percent = *low;
                settings.notifications.critical_battery_percent = *critical;
                settings.validate().unwrap();
            }
        }
    }
}

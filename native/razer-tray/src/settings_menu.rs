use std::error::Error;

use razer_core::settings::AppSettings;
use tray_icon::menu::{CheckMenuItem, MenuEvent, MenuItem, Submenu};

const POLL_INTERVAL_OPTIONS: &[(u64, &str)] = &[
    (5, "5 seconds"),
    (15, "15 seconds"),
    (30, "30 seconds"),
    (60, "1 minute"),
    (300, "5 minutes"),
];

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
    poll_interval_items: Vec<(u64, CheckMenuItem)>,
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

        let test_notification_item =
            MenuItem::with_id("settings.test-notification", "Send test notification", true, None);
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

        let poll_interval_menu =
            Submenu::with_id("settings.poll-interval", "Polling interval", true);
        let poll_interval_items = POLL_INTERVAL_OPTIONS
            .iter()
            .map(|(seconds, label)| {
                let item = CheckMenuItem::with_id(
                    format!("settings.poll-interval.{seconds}"),
                    label,
                    true,
                    settings.poll_interval_seconds == *seconds,
                    None,
                );
                poll_interval_menu.append(&item)?;
                Ok((*seconds, item))
            })
            .collect::<Result<Vec<_>, tray_icon::menu::Error>>()?;
        root.append(&poll_interval_menu)?;

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
            poll_interval_items,
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

    pub fn poll_interval_for_event(&self, event: &MenuEvent) -> Option<u64> {
        self.poll_interval_items
            .iter()
            .find_map(|(seconds, item)| (event.id == *item.id()).then_some(*seconds))
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

        for (seconds, item) in &self.poll_interval_items {
            item.set_checked(settings.poll_interval_seconds == *seconds);
        }

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
    fn all_polling_options_are_valid_core_settings() {
        for (seconds, _) in POLL_INTERVAL_OPTIONS {
            let settings = AppSettings {
                poll_interval_seconds: *seconds,
                ..AppSettings::default()
            };
            settings.validate().unwrap();
        }
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

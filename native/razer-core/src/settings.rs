use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 1;
pub const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 30;
pub const MIN_POLL_INTERVAL_SECONDS: u64 = 5;
pub const MAX_POLL_INTERVAL_SECONDS: u64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrimaryDevicePreference {
    Auto,
    ProductId(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub low_battery_percent: u8,
    pub critical_battery_percent: u8,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            low_battery_percent: 20,
            critical_battery_percent: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub version: u32,
    pub poll_interval_seconds: u64,
    pub primary_device: PrimaryDevicePreference,
    pub notifications: NotificationSettings,
    pub start_with_windows: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            poll_interval_seconds: DEFAULT_POLL_INTERVAL_SECONDS,
            primary_device: PrimaryDevicePreference::Auto,
            notifications: NotificationSettings::default(),
            start_with_windows: false,
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.version != SETTINGS_VERSION {
            return Err(SettingsError::Invalid(format!(
                "unsupported settings version: {}",
                self.version
            )));
        }

        if !(MIN_POLL_INTERVAL_SECONDS..=MAX_POLL_INTERVAL_SECONDS)
            .contains(&self.poll_interval_seconds)
        {
            return Err(SettingsError::Invalid(format!(
                "poll interval must be between {MIN_POLL_INTERVAL_SECONDS} and {MAX_POLL_INTERVAL_SECONDS} seconds"
            )));
        }

        if self.notifications.low_battery_percent > 100 {
            return Err(SettingsError::Invalid(
                "low battery threshold must be between 0 and 100".to_string(),
            ));
        }

        if self.notifications.critical_battery_percent > 100 {
            return Err(SettingsError::Invalid(
                "critical battery threshold must be between 0 and 100".to_string(),
            ));
        }

        if self.notifications.critical_battery_percent > self.notifications.low_battery_percent {
            return Err(SettingsError::Invalid(
                "critical battery threshold must not exceed the low battery threshold".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    ConfigDirectoryUnavailable,
}

impl Display for SettingsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "settings I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid settings JSON: {error}"),
            Self::Invalid(message) => formatter.write_str(message),
            Self::ConfigDirectoryUnavailable => {
                formatter.write_str("application config directory is unavailable")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Invalid(_) | Self::ConfigDirectoryUnavailable => None,
        }
    }
}

impl From<io::Error> for SettingsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn for_current_user() -> Result<Self, SettingsError> {
        let project_dirs = ProjectDirs::from("com", "crytropy", "RazerBatteryTaskbar")
            .ok_or(SettingsError::ConfigDirectoryUnavailable)?;

        Ok(Self::new(project_dirs.config_dir().join("settings.json")))
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<AppSettings, SettingsError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AppSettings::default());
            }
            Err(error) => return Err(SettingsError::Io(error)),
        };

        let settings: AppSettings = serde_json::from_str(&content)?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        settings.validate()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(settings)?;
        fs::write(&self.path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_settings_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir()
            .join(format!("razer-battery-settings-{}-{unique}", std::process::id()))
            .join("settings.json")
    }

    #[test]
    fn default_settings_are_valid() {
        AppSettings::default().validate().unwrap();
    }

    #[test]
    fn rejects_invalid_notification_threshold_order() {
        let mut settings = AppSettings::default();
        settings.notifications.low_battery_percent = 10;
        settings.notifications.critical_battery_percent = 20;

        assert!(matches!(
            settings.validate(),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn missing_settings_file_returns_defaults() {
        let path = temporary_settings_path();
        let store = SettingsStore::new(path);

        assert_eq!(store.load().unwrap(), AppSettings::default());
    }

    #[test]
    fn settings_round_trip_through_json_file() {
        let path = temporary_settings_path();
        let store = SettingsStore::new(path.clone());

        let settings = AppSettings {
            poll_interval_seconds: 45,
            primary_device: PrimaryDevicePreference::ProductId(0x00AB),
            notifications: NotificationSettings {
                enabled: true,
                low_battery_percent: 25,
                critical_battery_percent: 8,
            },
            start_with_windows: true,
            ..AppSettings::default()
        };

        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap(), settings);

        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

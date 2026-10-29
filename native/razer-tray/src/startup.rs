use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::Path;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "RazerBatteryTaskbar";
const MAX_RUN_COMMAND_CHARS: usize = 260;

#[derive(Debug)]
pub enum StartupError {
    Io(io::Error),
    Registry(String),
    CommandTooLong(usize),
    #[cfg(not(windows))]
    UnsupportedPlatform,
}

impl Display for StartupError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "startup path error: {error}"),
            Self::Registry(error) => write!(formatter, "startup registry error: {error}"),
            Self::CommandTooLong(length) => write!(
                formatter,
                "startup command is {length} UTF-16 characters; Windows Run entries support at most {MAX_RUN_COMMAND_CHARS}"
            ),
            #[cfg(not(windows))]
            Self::UnsupportedPlatform => {
                formatter.write_str("start-with-Windows is only available on Windows")
            }
        }
    }
}

impl Error for StartupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Registry(_) | Self::CommandTooLong(_) => None,
            #[cfg(not(windows))]
            Self::UnsupportedPlatform => None,
        }
    }
}

impl From<io::Error> for StartupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Command-line suffix written by previews before the Windows HID transport
/// became the default. Such entries are still recognised as "enabled" and are
/// replaced with the plain executable path the next time startup is enabled.
const LEGACY_HID_FLAG_SUFFIX: &str = " --experimental-hid-00b7";

pub fn build_startup_command(executable: &Path) -> Result<String, StartupError> {
    let command = format!("\"{}\"", executable.display());
    let length = command.encode_utf16().count();

    if length > MAX_RUN_COMMAND_CHARS {
        return Err(StartupError::CommandTooLong(length));
    }

    Ok(command)
}

/// Whether an existing Run entry refers to `executable`, either as the plain
/// quoted path or as a legacy entry carrying the retired opt-in flag.
pub fn registered_command_matches(command: &str, executable: &Path) -> bool {
    let expected = format!("\"{}\"", executable.display());
    command == expected
        || command
            .strip_suffix(LEGACY_HID_FLAG_SUFFIX)
            .is_some_and(|prefix| prefix == expected)
}

#[cfg(windows)]
pub fn is_enabled() -> Result<bool, StartupError> {
    use windows_registry::CURRENT_USER;

    let executable = std::env::current_exe()?;
    let key = match CURRENT_USER.open(RUN_KEY) {
        Ok(key) => key,
        Err(_) => return Ok(false),
    };

    match key.get_string(VALUE_NAME) {
        Ok(command) => Ok(registered_command_matches(&command, &executable)),
        Err(_) => Ok(false),
    }
}

#[cfg(not(windows))]
pub fn is_enabled() -> Result<bool, StartupError> {
    Err(StartupError::UnsupportedPlatform)
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), StartupError> {
    use windows_registry::CURRENT_USER;

    let key = CURRENT_USER
        .create(RUN_KEY)
        .map_err(|error| StartupError::Registry(error.to_string()))?;

    if enabled {
        let command = build_startup_command(&std::env::current_exe()?)?;
        key.set_string(VALUE_NAME, command)
            .map_err(|error| StartupError::Registry(error.to_string()))?;
    } else if key.get_string(VALUE_NAME).is_ok() {
        key.remove_value(VALUE_NAME)
            .map_err(|error| StartupError::Registry(error.to_string()))?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), StartupError> {
    Err(StartupError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_paths_with_spaces() {
        let command =
            build_startup_command(Path::new(r"C:\Program Files\RazerBattery\razer-tray.exe"))
                .unwrap();

        assert_eq!(command, r#""C:\Program Files\RazerBattery\razer-tray.exe""#);
    }

    #[test]
    fn startup_command_registers_only_the_executable_path() {
        let executable = Path::new(r"C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe");
        let command = build_startup_command(executable).unwrap();

        assert_eq!(
            command,
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe""#
        );
        assert!(!command.contains("--"));
    }

    #[test]
    fn startup_command_rejects_values_over_the_windows_run_limit() {
        let long_name = "a".repeat(MAX_RUN_COMMAND_CHARS + 1);
        let error = build_startup_command(Path::new(&long_name)).unwrap_err();

        assert!(matches!(error, StartupError::CommandTooLong(_)));

        let fits_exactly = "a".repeat(MAX_RUN_COMMAND_CHARS - 2);
        assert!(build_startup_command(Path::new(&fits_exactly)).is_ok());
    }

    #[test]
    fn registered_command_matches_current_executable() {
        let executable = Path::new(r"C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe");

        assert!(registered_command_matches(
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe""#,
            executable,
        ));
        assert!(!registered_command_matches(
            r#""C:\Elsewhere\RazerBatteryTaskbar-Native-Preview.exe""#,
            executable,
        ));
        assert!(!registered_command_matches(
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe" --placeholder-transport"#,
            executable,
        ));
    }

    #[test]
    fn legacy_run_entry_with_retired_hid_flag_still_counts_as_enabled() {
        // Entries written by earlier previews carried the opt-in flag. They must
        // read as "enabled" so the toggle can overwrite them with the plain path.
        let executable = Path::new(r"C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe");

        assert!(registered_command_matches(
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe" --experimental-hid-00b7"#,
            executable,
        ));
        assert_eq!(
            build_startup_command(executable).unwrap(),
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe""#,
            "re-enabling startup must write the plain path, not the legacy flag"
        );
    }
}

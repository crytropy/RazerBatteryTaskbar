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

/// Retired opt-in flag from the preview builds. On the command line it is
/// accepted as a no-op; in a Run entry written by a preview it marks a legacy
/// registration that still counts as "enabled" and is rewritten to the plain
/// executable path when the tray reconciles its startup state.
pub const LEGACY_HID_FLAG: &str = "--experimental-hid-00b7";

/// How an existing Run entry relates to the current executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisteredCommand {
    /// The plain quoted path this build writes.
    Current,
    /// The quoted path followed by the retired opt-in flag.
    Legacy,
    /// Another executable or an unknown command line.
    Other,
}

pub fn classify_registered_command(command: &str, executable: &Path) -> RegisteredCommand {
    let expected = format!("\"{}\"", executable.display());
    if command == expected {
        RegisteredCommand::Current
    } else if command
        .strip_suffix(LEGACY_HID_FLAG)
        .and_then(|prefix| prefix.strip_suffix(' '))
        .is_some_and(|prefix| prefix == expected)
    {
        RegisteredCommand::Legacy
    } else {
        RegisteredCommand::Other
    }
}

/// The plain command a legacy Run entry must be rewritten to, or `None` when
/// the entry needs no change (already plain, or not ours to touch).
pub fn migrated_command(command: &str, executable: &Path) -> Result<Option<String>, StartupError> {
    match classify_registered_command(command, executable) {
        RegisteredCommand::Legacy => build_startup_command(executable).map(Some),
        RegisteredCommand::Current | RegisteredCommand::Other => Ok(None),
    }
}

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
    classify_registered_command(command, executable) != RegisteredCommand::Other
}

/// Rewrite a legacy flagged Run entry for this executable to the plain path.
/// Returns `true` when an entry was rewritten. Entries that are already plain
/// or belong to another executable are left alone.
#[cfg(windows)]
pub fn migrate_legacy_entry() -> Result<bool, StartupError> {
    use windows_registry::CURRENT_USER;

    let executable = std::env::current_exe()?;
    let Ok(key) = CURRENT_USER.create(RUN_KEY) else {
        return Ok(false);
    };
    let Ok(command) = key.get_string(VALUE_NAME) else {
        return Ok(false);
    };

    match migrated_command(&command, &executable)? {
        Some(plain) => {
            key.set_string(VALUE_NAME, plain)
                .map_err(|error| StartupError::Registry(error.to_string()))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(not(windows))]
pub fn migrate_legacy_entry() -> Result<bool, StartupError> {
    Err(StartupError::UnsupportedPlatform)
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

    /// Spec #8: a flagged entry left by a preview is rewritten to the plain
    /// path as soon as the tray reconciles its startup state, not only on a
    /// later toggle. The registry step itself is Windows-only; the decision
    /// is pure and tested here.
    #[test]
    fn legacy_run_entry_is_rewritten_to_the_plain_path() {
        let executable = Path::new(r"C:\Razer Battery\RazerBatteryTaskbar-Native.exe");
        let plain = r#""C:\Razer Battery\RazerBatteryTaskbar-Native.exe""#;
        let legacy = r#""C:\Razer Battery\RazerBatteryTaskbar-Native.exe" --experimental-hid-00b7"#;
        let foreign = r#""C:\Elsewhere\RazerBatteryTaskbar-Native.exe" --experimental-hid-00b7"#;

        assert_eq!(
            classify_registered_command(legacy, executable),
            RegisteredCommand::Legacy
        );
        assert_eq!(
            classify_registered_command(plain, executable),
            RegisteredCommand::Current
        );
        assert_eq!(
            classify_registered_command(foreign, executable),
            RegisteredCommand::Other
        );

        assert_eq!(
            migrated_command(legacy, executable).unwrap(),
            Some(plain.to_string())
        );
        assert_eq!(migrated_command(plain, executable).unwrap(), None);
        assert_eq!(migrated_command(foreign, executable).unwrap(), None);
        assert_eq!(LEGACY_HID_FLAG, "--experimental-hid-00b7");
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

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

pub fn build_startup_command(
    executable: &Path,
    experimental_hid: bool,
) -> Result<String, StartupError> {
    let mut command = format!("\"{}\"", executable.display());
    if experimental_hid {
        command.push_str(" --experimental-hid-00b7");
    }
    let length = command.encode_utf16().count();

    if length > MAX_RUN_COMMAND_CHARS {
        return Err(StartupError::CommandTooLong(length));
    }

    Ok(command)
}

#[cfg(windows)]
pub fn is_enabled(experimental_hid: bool) -> Result<bool, StartupError> {
    use windows_registry::CURRENT_USER;

    let expected = build_startup_command(&std::env::current_exe()?, experimental_hid)?;
    let key = match CURRENT_USER.open(RUN_KEY) {
        Ok(key) => key,
        Err(_) => return Ok(false),
    };

    match key.get_string(VALUE_NAME) {
        Ok(command) => Ok(command == expected),
        Err(_) => Ok(false),
    }
}

#[cfg(not(windows))]
pub fn is_enabled(_experimental_hid: bool) -> Result<bool, StartupError> {
    Err(StartupError::UnsupportedPlatform)
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool, experimental_hid: bool) -> Result<(), StartupError> {
    use windows_registry::CURRENT_USER;

    let key = CURRENT_USER
        .create(RUN_KEY)
        .map_err(|error| StartupError::Registry(error.to_string()))?;

    if enabled {
        let command = build_startup_command(&std::env::current_exe()?, experimental_hid)?;
        key.set_string(VALUE_NAME, command)
            .map_err(|error| StartupError::Registry(error.to_string()))?;
    } else if key.get_string(VALUE_NAME).is_ok() {
        key.remove_value(VALUE_NAME)
            .map_err(|error| StartupError::Registry(error.to_string()))?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool, _experimental_hid: bool) -> Result<(), StartupError> {
    Err(StartupError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_paths_with_spaces() {
        let command = build_startup_command(
            Path::new(r"C:\Program Files\RazerBattery\razer-tray.exe"),
            false,
        )
        .unwrap();

        assert_eq!(command, r#""C:\Program Files\RazerBattery\razer-tray.exe""#);
    }

    #[test]
    fn startup_command_requires_explicit_opt_in_to_persist_experimental_hid() {
        let executable = Path::new(r"C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe");
        let default = build_startup_command(executable, false).unwrap();
        let experimental = build_startup_command(executable, true).unwrap();

        assert_eq!(
            default,
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe""#
        );
        assert_eq!(
            experimental,
            r#""C:\Razer Battery\RazerBatteryTaskbar-Native-Preview.exe" --experimental-hid-00b7"#
        );
        assert!(!default.contains("--experimental-hid-00b7"));
    }

    #[test]
    fn startup_command_rejects_values_over_the_windows_run_limit() {
        let long_name = "a".repeat(MAX_RUN_COMMAND_CHARS + 1);
        let error = build_startup_command(Path::new(&long_name), false).unwrap_err();

        assert!(matches!(error, StartupError::CommandTooLong(_)));

        // The opt-in flag counts toward the Windows command-line length.
        let fits_without_flag = "a".repeat(MAX_RUN_COMMAND_CHARS - 2);
        let executable = Path::new(&fits_without_flag);
        assert!(build_startup_command(executable, false).is_ok());
        assert!(matches!(
            build_startup_command(executable, true),
            Err(StartupError::CommandTooLong(_))
        ));
    }
}

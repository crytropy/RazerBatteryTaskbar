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
            Self::Registry(_) | Self::CommandTooLong(_) | Self::UnsupportedPlatform => None,
        }
    }
}

impl From<io::Error> for StartupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
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

#[cfg(windows)]
pub fn is_enabled() -> Result<bool, StartupError> {
    use windows_registry::CURRENT_USER;

    let expected = build_startup_command(&std::env::current_exe()?)?;
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
    fn startup_command_rejects_values_over_the_windows_run_limit() {
        let long_name = "a".repeat(MAX_RUN_COMMAND_CHARS + 1);
        let error = build_startup_command(Path::new(&long_name)).unwrap_err();

        assert!(matches!(error, StartupError::CommandTooLong(_)));
    }
}

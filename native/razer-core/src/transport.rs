use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::state::DeviceReading;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    pub message: String,
}

impl TransportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for TransportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for TransportError {}

pub trait BatteryTransport {
    fn name(&self) -> &'static str;
    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyTransport;

    impl BatteryTransport for EmptyTransport {
        fn name(&self) -> &'static str {
            "empty-test"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn transport_contract_is_ui_independent() {
        let mut transport = EmptyTransport;
        assert_eq!(transport.name(), "empty-test");
        assert!(transport.read_devices().unwrap().is_empty());
    }
}

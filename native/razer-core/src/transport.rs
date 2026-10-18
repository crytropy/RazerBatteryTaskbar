use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::state::DeviceReading;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    Unknown,
    Enumeration,
    Open,
    Send,
    Receive,
    InvalidResponse,
    /// The receiver answered, but reported that the device itself did not
    /// (busy, failure, or timeout status). Typical when a wireless mouse is
    /// idle/asleep or momentarily out of range.
    DeviceNotResponding,
    AmbiguousCollection,
    Configuration,
}

impl TransportErrorKind {
    /// Machine-readable, privacy-safe error code for frontend integrations.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Enumeration => "enumeration",
            Self::Open => "open",
            Self::Send => "send",
            Self::Receive => "receive",
            Self::InvalidResponse => "invalidResponse",
            Self::DeviceNotResponding => "deviceNotResponding",
            Self::AmbiguousCollection => "ambiguousCollection",
            Self::Configuration => "configuration",
        }
    }

    /// These errors occur only after the candidate device was enumerated.
    pub fn device_was_enumerated(self) -> bool {
        matches!(
            self,
            Self::Open
                | Self::Send
                | Self::Receive
                | Self::InvalidResponse
                | Self::DeviceNotResponding
                | Self::AmbiguousCollection
        )
    }

    /// The receiver is present and the protocol exchange was well-formed, but
    /// the wireless device simply did not answer. The last verified battery
    /// value stays meaningful (the device is most likely asleep), so it is
    /// retained instead of being cleared.
    pub fn retains_last_battery(self) -> bool {
        matches!(self, Self::DeviceNotResponding)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

impl TransportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::with_kind(TransportErrorKind::Unknown, message)
    }

    pub fn with_kind(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
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

    fn is_ready(&self) -> bool {
        true
    }

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
    fn error_codes_are_stable_and_do_not_expose_device_paths() {
        let error = TransportError::with_kind(
            TransportErrorKind::Receive,
            "unable to receive battery report",
        );
        assert_eq!(error.kind.as_str(), "receive");
        assert!(error.kind.device_was_enumerated());
        assert!(TransportErrorKind::AmbiguousCollection.device_was_enumerated());
        assert!(!TransportErrorKind::Enumeration.device_was_enumerated());
        assert_eq!(
            TransportErrorKind::DeviceNotResponding.as_str(),
            "deviceNotResponding"
        );
        assert!(TransportErrorKind::DeviceNotResponding.device_was_enumerated());
        assert!(TransportErrorKind::DeviceNotResponding.retains_last_battery());
        assert!(!TransportErrorKind::InvalidResponse.retains_last_battery());
        assert!(!TransportErrorKind::Receive.retains_last_battery());
        assert_eq!(
            TransportError::new("fallback").kind,
            TransportErrorKind::Unknown
        );
    }

    #[test]
    fn transport_contract_is_ui_independent() {
        let mut transport = EmptyTransport;
        assert_eq!(transport.name(), "empty-test");
        assert!(transport.is_ready());
        assert!(transport.read_devices().unwrap().is_empty());
    }
}

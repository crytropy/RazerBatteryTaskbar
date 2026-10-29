use std::error::Error;
use std::fmt::{Display, Formatter};

pub const REPORT_SIZE: usize = 90;
pub const DATA_SIZE: usize = 80;
pub const CRC_OFFSET: usize = 88;
pub const BATTERY_LEVEL_OFFSET: usize = 9;
pub const CHARGING_STATUS_OFFSET: usize = 9;
/// Command class shared by the battery and charging queries.
pub const POWER_COMMAND_CLASS: u8 = 0x07;
/// Command id of the battery level query (class `0x07`).
pub const BATTERY_LEVEL_COMMAND: u8 = 0x80;
/// Command id of the charging status query (class `0x07`).
pub const CHARGING_STATUS_COMMAND: u8 = 0x84;
const POWER_COMMAND_DATA_SIZE: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    ResponseTooShort(usize),
    InvalidChargingStatus(u8),
}

impl Display for ProtocolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResponseTooShort(length) => {
                write!(formatter, "Razer response is too short: {length} bytes")
            }
            Self::InvalidChargingStatus(value) => {
                write!(
                    formatter,
                    "Razer charging status byte is invalid: 0x{value:02X}"
                )
            }
        }
    }
}

impl Error for ProtocolError {}

pub fn calculate_crc(report: &[u8; REPORT_SIZE]) -> u8 {
    report[2..CRC_OFFSET]
        .iter()
        .fold(0u8, |crc, byte| crc ^ byte)
}

/// Build a 90-byte power-class (`0x07`) request frame for `command`.
///
/// Byte layout: `[0]` status, `[1]` transaction id, `[2..5]` reserved,
/// `[5]` data size, `[6]` command class, `[7]` command id, `[8..88]` arguments,
/// `[88]` CRC, `[89]` reserved.
pub fn build_request(transaction_id: u8, command: u8) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];

    report[0] = 0x00;
    report[1] = transaction_id;
    report[2] = 0x00;
    report[3] = 0x00;
    report[4] = 0x00;
    report[5] = POWER_COMMAND_DATA_SIZE;
    report[6] = POWER_COMMAND_CLASS;
    report[7] = command;
    report[CRC_OFFSET] = calculate_crc(&report);
    report[REPORT_SIZE - 1] = 0x00;

    report
}

pub fn build_battery_request(transaction_id: u8) -> [u8; REPORT_SIZE] {
    build_request(transaction_id, BATTERY_LEVEL_COMMAND)
}

pub fn build_charging_request(transaction_id: u8) -> [u8; REPORT_SIZE] {
    build_request(transaction_id, CHARGING_STATUS_COMMAND)
}

pub fn parse_battery_level(response: &[u8]) -> Result<f32, ProtocolError> {
    if response.len() <= BATTERY_LEVEL_OFFSET {
        return Err(ProtocolError::ResponseTooShort(response.len()));
    }

    Ok(response[BATTERY_LEVEL_OFFSET] as f32 / 255.0 * 100.0)
}

/// Parse the reply to a charging status query: `0x01` charging, `0x00` not.
pub fn parse_charging_status(response: &[u8]) -> Result<bool, ProtocolError> {
    if response.len() <= CHARGING_STATUS_OFFSET {
        return Err(ProtocolError::ResponseTooShort(response.len()));
    }

    match response[CHARGING_STATUS_OFFSET] {
        0x00 => Ok(false),
        0x01 => Ok(true),
        other => Err(ProtocolError::InvalidChargingStatus(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_same_90_byte_battery_request_as_the_compatibility_core() {
        let request = build_battery_request(0x1f);

        assert_eq!(request.len(), REPORT_SIZE);
        assert_eq!(request[1], 0x1f);
        assert_eq!(request[5], 0x02);
        assert_eq!(request[6], 0x07);
        assert_eq!(request[7], 0x80);
        assert_eq!(request[CRC_OFFSET], 0x85);
        assert_eq!(request[REPORT_SIZE - 1], 0x00);
    }

    #[test]
    fn parses_battery_percentage() {
        let mut response = [0u8; REPORT_SIZE];
        response[BATTERY_LEVEL_OFFSET] = 128;

        let battery = parse_battery_level(&response).unwrap();

        assert!((battery - 50.196_08).abs() < 0.000_1);
    }

    #[test]
    fn rejects_short_responses() {
        assert_eq!(
            parse_battery_level(&[0u8; 9]),
            Err(ProtocolError::ResponseTooShort(9)),
        );
    }

    #[test]
    fn charging_request_shares_the_battery_frame_and_differs_only_in_the_command() {
        let battery = build_battery_request(0x1f);
        let charging = build_charging_request(0x1f);

        assert_eq!(charging.len(), REPORT_SIZE);
        assert_eq!(charging[1], 0x1f);
        assert_eq!(charging[5], 0x02);
        assert_eq!(charging[6], 0x07);
        assert_eq!(charging[7], CHARGING_STATUS_COMMAND);
        assert_eq!(charging[7], 0x84);
        assert_eq!(charging[CRC_OFFSET], calculate_crc(&charging));
        assert_ne!(charging[CRC_OFFSET], battery[CRC_OFFSET]);
        assert_eq!(charging[REPORT_SIZE - 1], 0x00);

        // Everything except the command id and checksum is identical.
        for index in (0..REPORT_SIZE).filter(|i| *i != 7 && *i != CRC_OFFSET) {
            assert_eq!(charging[index], battery[index], "byte {index} differs");
        }
        assert_eq!(build_request(0x1f, BATTERY_LEVEL_COMMAND), battery);
    }

    #[test]
    fn parses_charging_status() {
        let mut response = [0u8; REPORT_SIZE];
        response[CHARGING_STATUS_OFFSET] = 0x01;
        assert_eq!(parse_charging_status(&response), Ok(true));

        response[CHARGING_STATUS_OFFSET] = 0x00;
        assert_eq!(parse_charging_status(&response), Ok(false));
    }

    #[test]
    fn rejects_invalid_charging_status() {
        let mut response = [0u8; REPORT_SIZE];
        response[CHARGING_STATUS_OFFSET] = 0x02;
        assert_eq!(
            parse_charging_status(&response),
            Err(ProtocolError::InvalidChargingStatus(0x02)),
        );
        assert_eq!(
            parse_charging_status(&[0u8; 9]),
            Err(ProtocolError::ResponseTooShort(9)),
        );
    }
}

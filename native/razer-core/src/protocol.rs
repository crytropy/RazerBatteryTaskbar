use std::error::Error;
use std::fmt::{Display, Formatter};

pub const REPORT_SIZE: usize = 90;
pub const DATA_SIZE: usize = 80;
pub const CRC_OFFSET: usize = 88;
pub const BATTERY_LEVEL_OFFSET: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    ResponseTooShort(usize),
}

impl Display for ProtocolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResponseTooShort(length) => {
                write!(formatter, "Razer response is too short: {length} bytes")
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

pub fn build_battery_request(transaction_id: u8) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];

    report[0] = 0x00;
    report[1] = transaction_id;
    report[2] = 0x00;
    report[3] = 0x00;
    report[4] = 0x00;
    report[5] = 0x02;
    report[6] = 0x07;
    report[7] = 0x80;
    report[CRC_OFFSET] = calculate_crc(&report);
    report[REPORT_SIZE - 1] = 0x00;

    report
}

pub fn parse_battery_level(response: &[u8]) -> Result<f32, ProtocolError> {
    if response.len() <= BATTERY_LEVEL_OFFSET {
        return Err(ProtocolError::ResponseTooShort(response.len()));
    }

    Ok(response[BATTERY_LEVEL_OFFSET] as f32 / 255.0 * 100.0)
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
}

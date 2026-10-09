use std::thread;
use std::time::Duration;

use hidapi::HidApi;

use crate::RAZER_VENDOR_ID;
use crate::device_db::get_product;
use crate::protocol::{
    CRC_OFFSET, REPORT_SIZE, build_battery_request, calculate_crc, parse_battery_level,
};
use crate::state::DeviceReading;
use crate::transport::{BatteryTransport, TransportError};

const PRODUCT_ID: u16 = 0x00B7;
const INTERFACE_NUMBER: i32 = 0;
const USAGE_PAGE: u16 = 0x0001;
const USAGE: u16 = 0x0002;
const SUCCESS_STATUS: u8 = 0x02;
const COMMAND_CLASS: u8 = 0x07;
const COMMAND_ID: u8 = 0x80;
const RESPONSE_DELAY: Duration = Duration::from_millis(500);

pub struct ExperimentalDeathAdderTransport;

impl ExperimentalDeathAdderTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExperimentalDeathAdderTransport {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_feature_reply(bytes: &[u8], transaction_id: u8) -> Result<f32, TransportError> {
    if bytes.len() < REPORT_SIZE + 1 {
        return Err(TransportError::new("short HID feature response"));
    }

    if bytes[0] != 0 {
        return Err(TransportError::new("unexpected HID report ID"));
    }

    let mut report = [0u8; REPORT_SIZE];
    report.copy_from_slice(&bytes[1..REPORT_SIZE + 1]);

    if report[0] != SUCCESS_STATUS {
        return Err(TransportError::new("Razer command did not report success"));
    }

    if report[1] != transaction_id {
        return Err(TransportError::new("Razer transaction ID mismatch"));
    }

    if report[6] != COMMAND_CLASS || report[7] != COMMAND_ID {
        return Err(TransportError::new("Razer response command mismatch"));
    }

    if calculate_crc(&report) != report[CRC_OFFSET] {
        return Err(TransportError::new("Razer response checksum mismatch"));
    }

    parse_battery_level(&report).map_err(|_| TransportError::new("battery level unavailable"))
}

impl BatteryTransport for ExperimentalDeathAdderTransport {
    fn name(&self) -> &'static str {
        "windows-hid-00b7-experimental"
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        let api = HidApi::new()
            .map_err(|_| TransportError::new("unable to initialize Windows HID enumeration"))?;

        let mut candidates = api.device_list().filter(|device| {
            device.vendor_id() == RAZER_VENDOR_ID
                && device.product_id() == PRODUCT_ID
                && device.interface_number() == INTERFACE_NUMBER
                && device.usage_page() == USAGE_PAGE
                && device.usage() == USAGE
        });

        let Some(candidate) = candidates.next() else {
            return Ok(Vec::new());
        };

        if candidates.next().is_some() {
            return Err(TransportError::new(
                "more than one matching 00B7 HID collection; refusing ambiguous selection",
            ));
        }

        let product = get_product(PRODUCT_ID)
            .ok_or_else(|| TransportError::new("00B7 missing from Razer device database"))?;

        let device = candidate
            .open_device(&api)
            .map_err(|_| TransportError::new("unable to open DeathAdder HID collection"))?;

        let request = build_battery_request(product.transaction_id);
        let mut feature_request = [0u8; REPORT_SIZE + 1];
        feature_request[1..].copy_from_slice(&request);

        device
            .send_feature_report(&feature_request)
            .map_err(|_| TransportError::new("Razer battery query could not be sent"))?;
        thread::sleep(RESPONSE_DELAY);

        let mut reply = [0u8; REPORT_SIZE + 1];
        let received = device
            .get_feature_report(&mut reply)
            .map_err(|_| TransportError::new("Razer battery response could not be read"))?;

        let battery = parse_feature_reply(&reply[..received], product.transaction_id)?;

        Ok(vec![DeviceReading {
            vendor_id: RAZER_VENDOR_ID,
            product_id: PRODUCT_ID,
            product_name: Some(product.name.clone()),
            device_type: product.device_type,
            battery: Some(battery),
            charging: None,
            serial_number: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(transaction_id: u8, battery: u8) -> [u8; REPORT_SIZE + 1] {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = SUCCESS_STATUS;
        report[1] = transaction_id;
        report[5] = 0x02;
        report[6] = COMMAND_CLASS;
        report[7] = COMMAND_ID;
        report[9] = battery;
        report[CRC_OFFSET] = calculate_crc(&report);

        let mut bytes = [0u8; REPORT_SIZE + 1];
        bytes[1..].copy_from_slice(&report);
        bytes
    }

    #[test]
    fn accepts_checksum_verified_hid_battery_reply() {
        let packet = reply(0x1f, 201);
        let battery = parse_feature_reply(&packet, 0x1f).unwrap();
        assert!((battery - (201.0 / 255.0 * 100.0)).abs() < 0.001);
    }

    #[test]
    fn rejects_wrong_report_id_transaction_command_and_checksum() {
        let valid = reply(0x1f, 201);
        assert!(parse_feature_reply(&valid[..REPORT_SIZE], 0x1f).is_err());
        assert!(parse_feature_reply(&valid, 0x3f).is_err());

        let mut incorrect = valid;
        incorrect[0] = 1;
        assert!(parse_feature_reply(&incorrect, 0x1f).is_err());

        let mut incorrect = valid;
        incorrect[7] = 0x08;
        assert!(parse_feature_reply(&incorrect, 0x1f).is_err());

        let mut incorrect = valid;
        incorrect[10] = 44;
        assert!(parse_feature_reply(&incorrect, 0x1f).is_err());
    }

    #[test]
    fn transport_identifies_experimental_profile_explicitly() {
        let transport = ExperimentalDeathAdderTransport::new();
        assert_eq!(transport.name(), "windows-hid-00b7-experimental");
        assert!(transport.is_ready());
    }
}

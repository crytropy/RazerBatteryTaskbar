use std::thread;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::RAZER_VENDOR_ID;
use crate::device_db::{LogicalDevice, get_product, logical_device};
use crate::protocol::{
    BATTERY_LEVEL_COMMAND, CHARGING_STATUS_COMMAND, CRC_OFFSET, POWER_COMMAND_CLASS, REPORT_SIZE,
    build_battery_request, build_charging_request, calculate_crc, parse_battery_level,
    parse_charging_status,
};
use crate::state::DeviceReading;
use crate::transport::{BatteryTransport, TransportError, TransportErrorKind};

/// The DeathAdder V3 Pro receiver: the one model validated on hardware. Its
/// wired twin (`00B6`) is resolved through the database pairing, so the
/// transport reads the mouse over the receiver or over its USB cable alike.
const SUPPORTED_PRODUCT_ID: u16 = 0x00B7;
const INTERFACE_NUMBER: i32 = 0;
const USAGE_PAGE: u16 = 0x0001;
const USAGE: u16 = 0x0002;
const SUCCESS_STATUS: u8 = 0x02;
/// Razer status bytes other than success, as documented by OpenRazer.
const STATUS_BUSY: u8 = 0x01;
const STATUS_FAILURE: u8 = 0x03;
const STATUS_TIMEOUT: u8 = 0x04;
const STATUS_NOT_SUPPORTED: u8 = 0x05;
const RESPONSE_DELAY: Duration = Duration::from_millis(500);

/// Native Windows HID feature-report transport (ADR-0001), reported as
/// `windows-hid`. It tracks one logical device and accepts every product ID
/// that device can enumerate as.
pub struct WindowsHidTransport {
    device: LogicalDevice,
}

impl WindowsHidTransport {
    pub fn new() -> Self {
        Self {
            device: logical_device(SUPPORTED_PRODUCT_ID)
                .expect("supported product ID must exist in the embedded Razer device database"),
        }
    }

    /// The logical device this transport reads, with its wired / wireless twins.
    pub fn supported_device(&self) -> &LogicalDevice {
        &self.device
    }

    /// Choose which enumerated product ID to query. The wired twin wins when
    /// both are present: on the cable the receiver usually keeps enumerating
    /// but answers `deviceNotResponding`.
    fn select_product_id(&self, present: &[u16]) -> Option<u16> {
        self.device
            .product_ids()
            .into_iter()
            .find(|product_id| present.contains(product_id))
    }

    /// Build the reading for the logical device, whichever twin answered.
    /// The canonical product ID and base model name keep the device id and
    /// the user-visible name identical across cable plug / unplug, so the
    /// manager never sees a second device or a disconnect.
    fn build_reading(&self, battery: f32, charging: Option<bool>) -> DeviceReading {
        DeviceReading {
            vendor_id: RAZER_VENDOR_ID,
            product_id: self.device.product_id,
            product_name: Some(self.device.name.clone()),
            device_type: self.device.device_type,
            battery: Some(battery),
            charging,
            serial_number: None,
        }
    }
}

impl Default for WindowsHidTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// Translate a non-success Razer status byte into a classified error whose
/// message always carries the raw status code for diagnostics.
pub fn classify_status(status: u8) -> TransportError {
    match status {
        STATUS_BUSY => TransportError::with_kind(
            TransportErrorKind::DeviceNotResponding,
            format!(
                "Razer status 0x{status:02X} (busy): device did not answer; it may be idle or asleep"
            ),
        ),
        STATUS_FAILURE => TransportError::with_kind(
            TransportErrorKind::DeviceNotResponding,
            format!(
                "Razer status 0x{status:02X} (failure): receiver could not reach the device; it may be asleep or out of range"
            ),
        ),
        STATUS_TIMEOUT => TransportError::with_kind(
            TransportErrorKind::DeviceNotResponding,
            format!("Razer status 0x{status:02X} (timeout): device did not answer in time"),
        ),
        STATUS_NOT_SUPPORTED => TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            format!(
                "Razer status 0x{status:02X} (not supported): device rejected the battery command"
            ),
        ),
        _ => TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            format!("Razer status 0x{status:02X}: command did not report success"),
        ),
    }
}

/// Validate the framing shared by every power-class reply (report id, status,
/// transaction id, command, checksum) and return the bare 90-byte report.
fn validate_feature_reply(
    bytes: &[u8],
    transaction_id: u8,
    command: u8,
) -> Result<[u8; REPORT_SIZE], TransportError> {
    if bytes.len() < REPORT_SIZE + 1 {
        return Err(TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "short HID feature response",
        ));
    }

    if bytes[0] != 0 {
        return Err(TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "unexpected HID report ID",
        ));
    }

    let mut report = [0u8; REPORT_SIZE];
    report.copy_from_slice(&bytes[1..REPORT_SIZE + 1]);

    if report[0] != SUCCESS_STATUS {
        return Err(classify_status(report[0]));
    }

    if report[1] != transaction_id {
        return Err(TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "Razer transaction ID mismatch",
        ));
    }

    if report[6] != POWER_COMMAND_CLASS || report[7] != command {
        return Err(TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "Razer response command mismatch",
        ));
    }

    if calculate_crc(&report) != report[CRC_OFFSET] {
        return Err(TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "Razer response checksum mismatch",
        ));
    }

    Ok(report)
}

/// Parse a validated battery level reply into a percentage.
pub fn parse_feature_reply(bytes: &[u8], transaction_id: u8) -> Result<f32, TransportError> {
    let report = validate_feature_reply(bytes, transaction_id, BATTERY_LEVEL_COMMAND)?;

    parse_battery_level(&report).map_err(|_| {
        TransportError::with_kind(
            TransportErrorKind::InvalidResponse,
            "battery level unavailable",
        )
    })
}

/// Parse a validated charging status reply (`0x01` charging, `0x00` not).
pub fn parse_charging_reply(bytes: &[u8], transaction_id: u8) -> Result<bool, TransportError> {
    let report = validate_feature_reply(bytes, transaction_id, CHARGING_STATUS_COMMAND)?;

    parse_charging_status(&report).map_err(|error| {
        TransportError::with_kind(TransportErrorKind::InvalidResponse, error.to_string())
    })
}

/// Send one 90-byte request as a feature report, wait for the device, and
/// read back the reply (report id included). `query_name` labels the query
/// ("battery", "charging status") in error messages.
fn query_feature(
    device: &HidDevice,
    request: &[u8; REPORT_SIZE],
    query_name: &str,
) -> Result<Vec<u8>, TransportError> {
    let mut feature_request = [0u8; REPORT_SIZE + 1];
    feature_request[1..].copy_from_slice(request);

    device.send_feature_report(&feature_request).map_err(|_| {
        TransportError::with_kind(
            TransportErrorKind::Send,
            format!("Razer {query_name} query could not be sent"),
        )
    })?;
    thread::sleep(RESPONSE_DELAY);

    let mut reply = [0u8; REPORT_SIZE + 1];
    let received = device.get_feature_report(&mut reply).map_err(|_| {
        TransportError::with_kind(
            TransportErrorKind::Receive,
            format!("Razer {query_name} response could not be read"),
        )
    })?;

    Ok(reply[..received].to_vec())
}

impl BatteryTransport for WindowsHidTransport {
    fn name(&self) -> &'static str {
        "windows-hid"
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        let api = HidApi::new().map_err(|_| {
            TransportError::with_kind(
                TransportErrorKind::Enumeration,
                "unable to initialize Windows HID enumeration",
            )
        })?;

        // One matching collection per product ID; both twins may be present
        // at once (mouse on the cable, receiver still plugged in).
        let product_ids = self.device.product_ids();
        let mut candidates: Vec<&hidapi::DeviceInfo> = Vec::new();

        for device in api.device_list().filter(|device| {
            device.vendor_id() == RAZER_VENDOR_ID
                && product_ids.contains(&device.product_id())
                && device.interface_number() == INTERFACE_NUMBER
                && device.usage_page() == USAGE_PAGE
                && device.usage() == USAGE
        }) {
            if candidates
                .iter()
                .any(|known| known.product_id() == device.product_id())
            {
                return Err(TransportError::with_kind(
                    TransportErrorKind::AmbiguousCollection,
                    format!(
                        "more than one matching {:04X} HID collection; refusing ambiguous selection",
                        device.product_id()
                    ),
                ));
            }

            candidates.push(device);
        }

        let present: Vec<u16> = candidates
            .iter()
            .map(|device| device.product_id())
            .collect();

        let Some(product_id) = self.select_product_id(&present) else {
            return Ok(Vec::new());
        };

        let candidate = candidates
            .iter()
            .find(|device| device.product_id() == product_id)
            .expect("selected product ID comes from the enumerated candidates");

        // Every product ID of the logical device comes from the database
        // pairing, so the lookup cannot fail for a selected candidate.
        let product = get_product(product_id)
            .expect("selected product ID comes from the embedded Razer device database");

        let device = candidate.open_device(&api).map_err(|_| {
            TransportError::with_kind(
                TransportErrorKind::Open,
                format!("unable to open {} HID collection", self.device.name),
            )
        })?;

        let transaction_id = product.transaction_id;
        let battery_reply =
            query_feature(&device, &build_battery_request(transaction_id), "battery")?;
        let battery = parse_feature_reply(&battery_reply, transaction_id)?;

        // The charging query reuses the open handle. It is best effort: a
        // failed or unanswered charging query leaves the verified battery
        // reading intact and only reports the charging state as unknown. It
        // must never turn into a transport error on its own.
        let charging = query_feature(
            &device,
            &build_charging_request(transaction_id),
            "charging status",
        )
        .and_then(|reply| parse_charging_reply(&reply, transaction_id))
        .ok();

        Ok(vec![self.build_reading(battery, charging)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DeviceType;

    fn command_reply(transaction_id: u8, command: u8, payload: u8) -> [u8; REPORT_SIZE + 1] {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = SUCCESS_STATUS;
        report[1] = transaction_id;
        report[5] = 0x02;
        report[6] = POWER_COMMAND_CLASS;
        report[7] = command;
        report[9] = payload;
        report[CRC_OFFSET] = calculate_crc(&report);

        let mut bytes = [0u8; REPORT_SIZE + 1];
        bytes[1..].copy_from_slice(&report);
        bytes
    }

    fn reply(transaction_id: u8, battery: u8) -> [u8; REPORT_SIZE + 1] {
        command_reply(transaction_id, BATTERY_LEVEL_COMMAND, battery)
    }

    #[test]
    fn charging_reply_is_validated_like_the_battery_reply() {
        let charging = command_reply(0x1f, CHARGING_STATUS_COMMAND, 0x01);
        assert_eq!(parse_charging_reply(&charging, 0x1f), Ok(true));

        let idle = command_reply(0x1f, CHARGING_STATUS_COMMAND, 0x00);
        assert_eq!(parse_charging_reply(&idle, 0x1f), Ok(false));

        let garbage = command_reply(0x1f, CHARGING_STATUS_COMMAND, 0x7f);
        let error = parse_charging_reply(&garbage, 0x1f).unwrap_err();
        assert_eq!(error.kind, TransportErrorKind::InvalidResponse);

        // A battery reply must not be mistaken for a charging reply and vice versa.
        let battery = reply(0x1f, 0x01);
        assert_eq!(
            parse_charging_reply(&battery, 0x1f).unwrap_err().kind,
            TransportErrorKind::InvalidResponse
        );
        assert_eq!(
            parse_feature_reply(&charging, 0x1f).unwrap_err().kind,
            TransportErrorKind::InvalidResponse
        );

        let mismatch = parse_charging_reply(&charging, 0x3f).unwrap_err();
        assert_eq!(mismatch.kind, TransportErrorKind::InvalidResponse);

        let mut corrupt = charging;
        corrupt[10] = 44;
        assert!(parse_charging_reply(&corrupt, 0x1f).is_err());

        let mut asleep = charging;
        asleep[1] = STATUS_FAILURE;
        assert_eq!(
            parse_charging_reply(&asleep, 0x1f).unwrap_err().kind,
            TransportErrorKind::DeviceNotResponding
        );
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
        let mismatch = parse_feature_reply(&valid, 0x3f).unwrap_err();
        assert_eq!(mismatch.kind, TransportErrorKind::InvalidResponse);

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
    fn non_success_status_is_classified_and_reported_in_the_message() {
        let valid = reply(0x1f, 201);

        for status in [STATUS_BUSY, STATUS_FAILURE, STATUS_TIMEOUT] {
            let mut asleep = valid;
            asleep[1] = status;
            let error = parse_feature_reply(&asleep, 0x1f).unwrap_err();
            assert_eq!(error.kind, TransportErrorKind::DeviceNotResponding);
            assert!(
                error.message.contains(&format!("0x{status:02X}")),
                "message should carry the raw status: {}",
                error.message
            );
        }

        let mut unsupported = valid;
        unsupported[1] = STATUS_NOT_SUPPORTED;
        let error = parse_feature_reply(&unsupported, 0x1f).unwrap_err();
        assert_eq!(error.kind, TransportErrorKind::InvalidResponse);
        assert!(error.message.contains("0x05"));

        let mut unknown = valid;
        unknown[1] = 0x7E;
        let error = parse_feature_reply(&unknown, 0x1f).unwrap_err();
        assert_eq!(error.kind, TransportErrorKind::InvalidResponse);
        assert!(error.message.contains("0x7E"));
    }

    #[test]
    fn transport_reports_a_stable_non_experimental_name() {
        let transport = WindowsHidTransport::new();
        assert_eq!(transport.name(), "windows-hid");
        assert!(transport.is_ready());
    }

    #[test]
    fn transport_reads_both_product_ids_of_the_deathadder_v3_pro() {
        let transport = WindowsHidTransport::new();
        let device = transport.supported_device();

        assert_eq!(device.name, "Razer DeathAdder V3 Pro");
        assert_eq!(device.product_ids(), vec![0x00B6, 0x00B7]);
    }

    #[test]
    fn wired_twin_is_preferred_when_both_are_enumerated() {
        let transport = WindowsHidTransport::new();

        // Receiver still enumerated while the mouse is on the cable: read the cable.
        assert_eq!(transport.select_product_id(&[0x00B7, 0x00B6]), Some(0x00B6));
        assert_eq!(transport.select_product_id(&[0x00B6, 0x00B7]), Some(0x00B6));
        // Single product ID present: read whichever one it is.
        assert_eq!(transport.select_product_id(&[0x00B7]), Some(0x00B7));
        assert_eq!(transport.select_product_id(&[0x00B6]), Some(0x00B6));
        // Nothing (or only foreign hardware) enumerated: no device.
        assert_eq!(transport.select_product_id(&[]), None);
        assert_eq!(transport.select_product_id(&[0x00AB]), None);
    }

    #[test]
    fn reading_describes_the_logical_device_not_the_answering_twin() {
        let transport = WindowsHidTransport::new();

        let reading = transport.build_reading(80.0, Some(true));

        assert_eq!(reading.vendor_id, RAZER_VENDOR_ID);
        // Canonical identity is the receiver PID, whether the mouse answered
        // over the receiver or over its cable; the name has no link suffix.
        assert_eq!(reading.product_id, 0x00B7);
        assert_eq!(
            reading.product_name.as_deref(),
            Some("Razer DeathAdder V3 Pro")
        );
        assert_eq!(reading.device_type, DeviceType::Mouse);
        assert_eq!(reading.battery, Some(80.0));
        assert_eq!(reading.charging, Some(true));
        assert_eq!(reading.serial_number, None);
    }
}

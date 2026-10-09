#[cfg(all(windows, feature = "windows-hid"))]
mod test_app {
    use std::env;
    use std::error::Error;
    use std::io;
    use std::thread;
    use std::time::Duration;

    use hidapi::HidApi;
    use razer_core::RAZER_VENDOR_ID;
    use razer_core::device_db::get_product;
    use razer_core::protocol::{
        REPORT_SIZE, build_battery_request, calculate_crc, parse_battery_level,
    };

    const PRODUCT_ID: u16 = 0x00B7;
    const INTERFACE_NUMBER: i32 = 0;
    const USAGE_PAGE: u16 = 0x0001;
    const USAGE: u16 = 0x0002;
    const REQUEST_CLASS: u8 = 0x07;
    const REQUEST_ID: u8 = 0x80;
    const SUCCESS_STATUS: u8 = 0x02;
    const RESPONSE_DELAY: Duration = Duration::from_millis(500);

    fn invalid_response(message: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message)
    }

    fn parse_response(bytes: &[u8], transaction_id: u8) -> Result<f32, Box<dyn Error>> {
        if bytes.len() < REPORT_SIZE + 1 {
            return Err(invalid_response(
                "short HID feature report: expected report ID plus 90 bytes",
            )
            .into());
        }

        if bytes[0] != 0 {
            return Err(invalid_response("unexpected HID feature report ID").into());
        }

        let report = &bytes[1..1 + REPORT_SIZE];

        if report[0] != SUCCESS_STATUS {
            return Err(invalid_response(
                "Razer battery request was not acknowledged with success status 0x02",
            )
            .into());
        }

        if report[1] != transaction_id {
            return Err(
                invalid_response("Razer reply transaction ID does not match request").into(),
            );
        }

        if report[6] != REQUEST_CLASS || report[7] != REQUEST_ID {
            return Err(
                invalid_response("Razer reply command class or command ID does not match").into(),
            );
        }

        let mut full_report = [0u8; REPORT_SIZE];
        full_report.copy_from_slice(report);

        if calculate_crc(&full_report) != full_report[88] {
            return Err(invalid_response("Razer reply checksum mismatch").into());
        }

        Ok(parse_battery_level(report)?)
    }

    fn read_once() -> Result<f32, Box<dyn Error>> {
        let api = HidApi::new()?;
        let mut candidates = api.device_list().filter(|device| {
            device.vendor_id() == RAZER_VENDOR_ID
                && device.product_id() == PRODUCT_ID
                && device.interface_number() == INTERFACE_NUMBER
                && device.usage_page() == USAGE_PAGE
                && device.usage() == USAGE
        });

        let selected = candidates.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "expected DeathAdder V3 Pro receiver collection MI_00 / 0001:0002 was not found",
            )
        })?;

        if candidates.next().is_some() {
            return Err(invalid_response(
                "multiple matching HID collections; refusing to choose an ambiguous device",
            )
            .into());
        }

        let transaction_id = get_product(PRODUCT_ID)
            .ok_or_else(|| invalid_response("0x00B7 is missing from the shared device database"))?
            .transaction_id;

        // This opens only the known candidate collection; no paths are logged.
        let device = selected.open_device(&api)?;

        // HIDAPI requires an explicit report-ID byte before the 90-byte Razer payload.
        let request = build_battery_request(transaction_id);
        let mut feature_request = [0u8; REPORT_SIZE + 1];
        feature_request[1..].copy_from_slice(&request);

        // A SetFeature query is necessary even for a read-only battery operation.
        device.send_feature_report(&feature_request)?;
        thread::sleep(RESPONSE_DELAY);

        let mut feature_reply = [0u8; REPORT_SIZE + 1];
        let received = device.get_feature_report(&mut feature_reply)?;
        parse_response(&feature_reply[..received], transaction_id)
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        match env::args().nth(1).as_deref() {
            Some("--read-battery-once") if env::args().len() == 2 => {}
            Some("--help") | Some("-h") if env::args().len() == 2 => {
                println!(
                    "RazerBatteryTaskbar Battery Once Test\n\n\
                    Usage:\n  RazerBatteryTaskbar-Battery-Once-Test.exe --read-battery-once\n\n\
                    This test opens only PID 00B7, MI_00, usage 0001:0002 and sends\n\
                    ONE Razer battery query (SetFeature/GetFeature).\n\
                    It does not change DPI, polling rate, lighting, or device settings.\n\
                    Quit Razer Synapse and its background services before the first test.\n\
                    No HID paths or serial numbers are printed."
                );
                return Ok(());
            }
            _ => {
                eprintln!(
                    "No HID request sent. Use --help first, then explicitly pass --read-battery-once."
                );
                std::process::exit(2);
            }
        }

        println!("Testing DeathAdder V3 Pro receiver (1532:00B7), MI_00 / usage 0001:0002.");
        println!("One battery query will be sent. No device settings will be changed.");

        match read_once() {
            Ok(battery) => {
                println!("Battery: {battery:.1}%");
                println!("Result: acknowledged and checksum verified");
                Ok(())
            }
            Err(error) => {
                eprintln!("Battery query could not be verified: {error}");
                Err(error)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn good_feature_reply(transaction_id: u8, battery: u8) -> [u8; REPORT_SIZE + 1] {
            let mut response = [0u8; REPORT_SIZE + 1];
            let mut packet = [0u8; REPORT_SIZE];
            packet[0] = SUCCESS_STATUS;
            packet[1] = transaction_id;
            packet[5] = 0x02;
            packet[6] = REQUEST_CLASS;
            packet[7] = REQUEST_ID;
            packet[9] = battery;
            packet[88] = calculate_crc(&packet);
            response[1..].copy_from_slice(&packet);
            response
        }

        #[test]
        fn parses_verified_battery_with_hid_report_id_prefix() {
            let report = good_feature_reply(0x1f, 128);
            let battery = parse_response(&report, 0x1f).unwrap();
            assert!((battery - 50.196_08).abs() < 0.000_1);
        }

        #[test]
        fn rejects_wrong_transaction_and_checksum() {
            let mut report = good_feature_reply(0x1f, 128);
            assert!(parse_response(&report, 0x3f).is_err());
            report[10] = 90;
            assert!(parse_response(&report, 0x1f).is_err());
        }

        #[test]
        fn rejects_short_report_or_unsupported_status() {
            let mut report = good_feature_reply(0x1f, 128);
            assert!(parse_response(&report[..REPORT_SIZE], 0x1f).is_err());
            report[1] = 0x05;
            assert!(parse_response(&report, 0x1f).is_err());
        }
    }
}

#[cfg(all(windows, feature = "windows-hid"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    test_app::run()
}

#[cfg(not(all(windows, feature = "windows-hid")))]
fn main() {
    eprintln!("This is a Windows-only HIDAPI diagnostic; requires --features windows-hid.");
    std::process::exit(2);
}

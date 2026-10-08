#[cfg(all(windows, feature = "windows-hid"))]
mod probe {
    use std::env;
    use std::error::Error;

    use hidapi::HidApi;
    use razer_core::RAZER_VENDOR_ID;
    use razer_core::device_db::get_product;
    use serde::Serialize;

    const HELP: &str = "RazerBatteryTaskbar HID Probe

Usage:
  RazerBatteryTaskbar-HID-Probe.exe [--json] [--known-only] [--pid 0xNNNN]
  RazerBatteryTaskbar-HID-Probe.exe --help

Options:
  --json          Print a machine-readable JSON report.
  --known-only    Show only product IDs in the shared Razer database.
  --pid 0xNNNN    Filter by a hexadecimal USB product ID.
  -h, --help      Show this help without initializing HID.

Read-only enumeration: does not open HID devices, send feature reports,
or include serial numbers, device paths, or internal identifiers.";

    #[derive(Debug, Default, PartialEq, Eq)]
    struct Options {
        json: bool,
        known_only: bool,
        product_id: Option<u16>,
    }

    #[derive(Debug, Clone, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    struct Collection {
        vendor_id: u16,
        product_id: u16,
        interface_number: i32,
        usage_page: u16,
        usage: u16,
        known_product: bool,
        product_name: String,
    }

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ProbeReport {
        schema_version: u8,
        read_only: bool,
        vendor_id: u16,
        collection_count: usize,
        collections: Vec<Collection>,
    }

    fn parse_pid(input: &str) -> Result<u16, String> {
        let digits = input
            .strip_prefix("0x")
            .or_else(|| input.strip_prefix("0X"))
            .ok_or_else(|| "product ID must start with 0x".to_string())?;

        if digits.is_empty() || digits.len() > 4 {
            return Err("product ID must contain 1 to 4 hexadecimal digits".to_string());
        }

        u16::from_str_radix(digits, 16)
            .map_err(|_| format!("invalid hexadecimal product ID: {input}"))
    }

    fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
        let mut options = Options::default();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json" => options.json = true,
                "--known-only" => options.known_only = true,
                "--pid" => {
                    if options.product_id.is_some() {
                        return Err("--pid may only be specified once".to_string());
                    }
                    let value = args
                        .next()
                        .ok_or_else(|| "--pid requires a hexadecimal product ID".to_string())?;
                    options.product_id = Some(parse_pid(&value)?);
                }
                "-h" | "--help" => return Ok(None),
                _ => return Err(format!("unrecognized argument: {arg}")),
            }
        }

        Ok(Some(options))
    }

    fn build_report(api: &HidApi, options: &Options) -> ProbeReport {
        let mut collections = Vec::new();

        for device in api
            .device_list()
            .filter(|device| device.vendor_id() == RAZER_VENDOR_ID)
        {
            if options
                .product_id
                .is_some_and(|product_id| device.product_id() != product_id)
            {
                continue;
            }

            let product = get_product(device.product_id());

            if options.known_only && product.is_none() {
                continue;
            }

            collections.push(Collection {
                vendor_id: device.vendor_id(),
                product_id: device.product_id(),
                interface_number: device.interface_number(),
                usage_page: device.usage_page(),
                usage: device.usage(),
                known_product: product.is_some(),
                product_name: product
                    .map(|definition| definition.name.clone())
                    .unwrap_or_else(|| "Unlisted Razer product".to_string()),
            });
        }

        collections.sort_by_key(|entry| {
            (
                entry.product_id,
                entry.interface_number,
                entry.usage_page,
                entry.usage,
            )
        });

        ProbeReport {
            schema_version: 1,
            read_only: true,
            vendor_id: RAZER_VENDOR_ID,
            collection_count: collections.len(),
            collections,
        }
    }

    fn print_text(report: &ProbeReport) {
        println!("Razer HID enumeration probe (read-only)");
        println!("No HID devices are opened and no feature reports are sent.");
        println!("Serial numbers and device paths are not collected.");
        println!();

        for (index, entry) in report.collections.iter().enumerate() {
            let known = if entry.known_product {
                "database match"
            } else {
                "unlisted"
            };

            println!(
                "#{}: VID={:04X} PID={:04X} interface={} usage_page={:04X} usage={:04X} [{}] {}",
                index + 1,
                entry.vendor_id,
                entry.product_id,
                entry.interface_number,
                entry.usage_page,
                entry.usage,
                known,
                entry.product_name,
            );
        }

        println!();
        println!("Razer HID collections found: {}", report.collection_count);
        println!("A database match does not confirm that native battery reads work.");
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let options = match parse_options(env::args().skip(1)) {
            Ok(Some(options)) => options,
            Ok(None) => {
                println!("{HELP}");
                return Ok(());
            }
            Err(message) => {
                eprintln!("{message}\n\n{HELP}");
                std::process::exit(2);
            }
        };

        let api = HidApi::new()?;
        let report = build_report(&api, &options);

        if options.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_text(&report);
        }

        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn parse(args: &[&str]) -> Result<Option<Options>, String> {
            parse_options(args.iter().map(|arg| (*arg).to_string()))
        }

        #[test]
        fn parses_hex_pid_and_combined_filters() {
            assert_eq!(
                parse(&["--json", "--pid", "0x00AB", "--known-only"]).unwrap(),
                Some(Options {
                    json: true,
                    known_only: true,
                    product_id: Some(0x00AB),
                })
            );
        }

        #[test]
        fn rejects_invalid_and_repeated_pid_arguments() {
            assert!(parse(&["--pid"]).is_err());
            assert!(parse(&["--pid", "171"]).is_err());
            assert!(parse(&["--pid", "0xGGGG"]).is_err());
            assert!(parse(&["--pid", "0x10000"]).is_err());
            assert!(parse(&["--pid", "0x00AB", "--pid", "0x0083"]).is_err());
            assert!(parse(&["--unknown"]).is_err());
        }

        #[test]
        fn help_does_not_require_hid_initialization() {
            assert_eq!(parse(&["--help"]).unwrap(), None);
        }

        #[test]
        fn json_report_excludes_private_hid_identifiers() {
            let report = ProbeReport {
                schema_version: 1,
                read_only: true,
                vendor_id: RAZER_VENDOR_ID,
                collection_count: 1,
                collections: vec![Collection {
                    vendor_id: RAZER_VENDOR_ID,
                    product_id: 0x00AB,
                    interface_number: 2,
                    usage_page: 1,
                    usage: 2,
                    known_product: true,
                    product_name: "Razer Basilisk V3 Pro Wireless".to_string(),
                }],
            };

            let json = serde_json::to_string(&report).unwrap();

            assert!(json.contains(r#""schemaVersion":1"#));
            assert!(json.contains(r#""collectionCount":1"#));
            assert!(json.contains(r#""knownProduct":true"#));
            assert!(!json.contains("serial"));
            assert!(!json.contains("path"));
        }
    }
}

#[cfg(all(windows, feature = "windows-hid"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run()
}

#[cfg(not(all(windows, feature = "windows-hid")))]
fn main() {
    eprintln!(
        "hid-probe is a Windows-only diagnostic. Build it with: \
         cargo run -p razer-core --example hid-probe --features windows-hid"
    );
    std::process::exit(2);
}

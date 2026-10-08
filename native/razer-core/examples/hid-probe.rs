#[cfg(all(windows, feature = "windows-hid"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hidapi::HidApi;
    use razer_core::RAZER_VENDOR_ID;

    let api = HidApi::new()?;
    let mut count = 0usize;

    println!("Razer HID enumeration probe");
    println!("No feature reports are sent and serial numbers are not printed.");
    println!();

    for device in api
        .device_list()
        .filter(|device| device.vendor_id() == RAZER_VENDOR_ID)
    {
        count += 1;

        println!(
            "#{count}: VID={:04X} PID={:04X} interface={} usage_page={:04X} usage={:04X} product={}",
            device.vendor_id(),
            device.product_id(),
            device.interface_number(),
            device.usage_page(),
            device.usage(),
            device.product_string().unwrap_or("<unknown>"),
        );
    }

    println!();
    println!("Razer HID collections found: {count}");

    Ok(())
}

#[cfg(not(all(windows, feature = "windows-hid")))]
fn main() {
    eprintln!(
        "hid-probe is a Windows-only diagnostic. Build it with: \
         cargo run -p razer-core --example hid-probe --features windows-hid"
    );
    std::process::exit(2);
}

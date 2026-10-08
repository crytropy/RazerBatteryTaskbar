# Rust core migration

Phase 8 starts as a parallel, UI-independent core. The existing Electron application remains the usable compatibility implementation while Rust components are proven independently.

## Initial Rust scope

The first Rust crate contains:

- The 90-byte Razer report format.
- XOR CRC generation.
- Battery request construction.
- Battery response parsing.
- A Windows-only HID enumeration probe.

The HID probe is intentionally read-only at this stage. It enumerates HID collections for vendor ID `0x1532` and prints PID, interface number, usage page, usage, and product string. It does not send feature reports and does not print device serial numbers.

## Why HID is being evaluated

The current Electron compatibility transport uses node-usb/libusb. The long-term Windows transport should preserve the normal Windows/Razer driver stack and avoid asking users to replace device drivers.

The Rust `hidapi` crate provides a Windows-native backend and feature-report APIs. The probe exists to establish which Razer HID collection/interface should be used before battery reads are implemented.

## Migration rule

Rust must preserve the external contracts already established in JavaScript:

- normalized device state;
- multi-device management;
- connection/battery/charging events;
- transport isolation;
- UI independence.

No Seelen-specific code belongs in the Rust core.

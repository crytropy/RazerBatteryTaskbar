# Windows transport strategy

## Current compatibility transport

The compatibility implementation uses node-usb WebUSB for enumeration and control transfers. The transport is isolated behind `src/transport/webusb-transport.js` so it can be replaced without changing the device manager, event contract, tray, notification system, or future integrations.

The transport also listens for node-usb attach/detach events and emits one debounced transport change event for supported Razer hardware. A 30-second polling cycle remains as a fallback.

## Reconnect behavior

- Supported USB attach/detach events request an immediate rescan.
- Windows resume requests a rescan after a short settle delay.
- A transport-wide enumeration failure retries after 2 seconds.
- Existing devices are only marked disconnected after three consecutive enumeration failures.
- A normal successful scan that returns no device still disconnects missing devices immediately.

This distinction avoids turning temporary resume/driver errors into false disconnects.

## Driver policy

The application must not automatically replace a Razer device driver or instruct users to use Zadig as part of normal installation.

node-usb/libusb can require WinUSB-compatible access on Windows and driver ownership can conflict with the normal vendor driver. The current Electron transport is therefore treated as a compatibility layer rather than the final Windows transport.

Before the Rust migration is finalized, HIDAPI / native Windows HID access should be validated against the supported Razer devices and Razer Synapse. The goal is to preserve the normal Razer driver stack and avoid requiring users to modify device drivers.

## Phase 7 exit criteria

Phase 7 is complete when:

1. USB transport is replaceable behind a stable boundary.
2. Attach/detach and Windows resume trigger fast reconnects.
3. Temporary enumeration failures do not immediately erase state.
4. Real hardware testing confirms behavior with and without Razer Synapse.
5. A final transport choice for the Rust core is documented.

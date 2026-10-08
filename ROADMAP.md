# RazerBatteryTaskbar modernization roadmap

The project is being modernized incrementally. Compatibility and a clean core architecture take priority over adding new UI integrations early.

## Architecture rules

1. Core device/protocol logic must not depend on any UI framework.
2. UI integrations must not talk to USB/HID devices directly.
3. The Windows tray must not parse the Razer protocol itself.
4. Optional integrations such as Seelen UI must be removable without affecting the core application.

## Milestones

| Phase | Goal | Status |
| --- | --- | --- |
| 0 | Baseline current behavior, supported devices, and known issues | Complete |
| 1 | Fix type handling, disconnect state, USB cleanup, and polling reliability | Complete |
| 2 | Split the Electron prototype into device/protocol/transport/UI modules | Complete |
| 3 | Introduce a normalized device state model | Complete |
| 4 | Move supported hardware into a maintainable device database | Complete |
| 5 | Add multi-device support | Complete |
| 6 | Add a device event system | Next |
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | Planned |
| 8 | Move the stable core to Rust | Planned |
| 9 | Build the lightweight Windows tray frontend | Planned |
| 10 | Add persistent settings | Planned |
| 11 | Add low/critical battery notifications | Planned |
| 12 | Add an optional integration API/IPC boundary | Planned |
| 13 | Add optional Seelen UI integration | Planned |
| 14 | Add automated tests and CI | In progress |
| 15 | Package and release a stable v1.0 of the rewritten application | Planned |

## Current architecture

The Electron application is still the runtime during the compatibility phase, but responsibilities are now separated:

- `src/core/device-state.js` owns normalized per-device application state.
- `src/core/device-manager.js` owns the collection of known devices and primary-device selection.
- `src/devices/razer-products.json` is the maintainable supported-device database.
- `src/devices/razer-products.js` validates and exposes that database.
- `src/protocol/razer-protocol.js` builds/parses Razer battery protocol messages.
- `src/usb/razer-battery-reader.js` enumerates supported WebUSB devices and queries them sequentially.
- `src/ui/tray-controller.js` renders the primary device and lists all known devices in the tray menu.
- `src/main.js` coordinates polling and application lifecycle.

Regression tests cover the state model, device manager, device database, and protocol. CI runs tests before publishing the Windows development build.

## Multi-device behavior

Each poll enumerates all supported Razer USB devices. Battery queries are performed sequentially so one device does not block the others. A detected device whose battery query fails remains connected with an unavailable battery value, while devices missing from a later scan are retained as disconnected states.

The tray icon follows a primary device. Until a user-selectable primary device is added in the settings phase, the manager prefers a connected device with a readable battery and uses a deterministic device-type/name ordering.

## Next phase

Phase 6 will introduce events such as DeviceConnected, DeviceDisconnected, BatteryChanged, and ChargingChanged so tray, notifications, APIs, and future Seelen UI integration can subscribe without coupling themselves to the polling loop.

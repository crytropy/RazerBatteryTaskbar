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
| 6 | Add a device event system | Complete |
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | Next |
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
- `src/core/device-manager.js` owns the collection of known devices, primary-device selection, and state transition detection.
- `src/core/device-events.js` defines the UI-independent event contract.
- `src/devices/razer-products.json` is the maintainable supported-device database.
- `src/devices/razer-products.js` validates and exposes that database.
- `src/protocol/razer-protocol.js` builds/parses Razer battery protocol messages.
- `src/usb/razer-battery-reader.js` enumerates supported WebUSB devices and queries them sequentially.
- `src/ui/tray-controller.js` renders the primary device and lists all known devices in the tray menu.
- `src/main.js` coordinates polling and application lifecycle.

Regression tests cover the state model, device manager, event transitions, device database, and protocol. CI runs tests before publishing the Windows development build.

## Event contract

The core now emits:

- `device-connected`
- `device-disconnected`
- `battery-changed`
- `charging-changed`
- `devices-changed`

Device-specific events carry immutable `previous` and `current` state references. The aggregate `devices-changed` event carries the latest device list and selected primary device.

The tray subscribes to the aggregate event rather than being called directly by the polling loop. Future notification, API, and Seelen integrations can subscribe to the same core events without changing USB transport code.

## Next phase

Phase 7 will evaluate and harden the Windows transport layer. The immediate goals are hotplug/reconnect behavior, transport isolation, and validating coexistence with Razer Synapse before committing to the later Rust transport implementation.

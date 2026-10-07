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
| 5 | Add multi-device support | Next |
| 6 | Add a device event system | Planned |
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

- `src/core/device-state.js` owns normalized application device state.
- `src/devices/razer-products.json` is the maintainable supported-device database.
- `src/devices/razer-products.js` validates and exposes that database.
- `src/protocol/razer-protocol.js` builds/parses Razer battery protocol messages.
- `src/usb/razer-battery-reader.js` owns WebUSB transport access.
- `src/ui/tray-controller.js` only renders normalized state.
- `src/main.js` coordinates polling and application lifecycle.

Basic regression tests now cover the state model, device database, and protocol. Full CI coverage remains part of Phase 14.

## Next phase

Phase 5 will replace the single-device assumption with a device collection and per-device state tracking while preserving the current tray behavior as the default presentation.

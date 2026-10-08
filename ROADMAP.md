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
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | Hardware validation |
| 8 | Move the stable core to Rust | Core contracts migrated |
| 9 | Build the lightweight Windows tray frontend | Planned |
| 10 | Add persistent settings | Planned |
| 11 | Add low/critical battery notifications | Planned |
| 12 | Add an optional integration API/IPC boundary | Planned |
| 13 | Add optional Seelen UI integration | Planned |
| 14 | Add automated tests and CI | In progress |
| 15 | Package and release a stable v1.0 of the rewritten application | Planned |

## Current architecture

The Electron application remains the usable compatibility runtime while the Rust core is developed in parallel.

JavaScript compatibility stack:

- `src/core/device-state.js` owns normalized per-device application state.
- `src/core/device-manager.js` owns the known-device collection and state transitions.
- `src/core/device-events.js` defines the UI-independent event contract.
- `src/transport/webusb-transport.js` isolates WebUSB enumeration and hotplug monitoring.
- `src/usb/razer-battery-reader.js` performs the current Razer control transfers.
- `src/ui/tray-controller.js` renders normalized state only.
- `src/main.js` coordinates lifecycle, refresh scheduling, diagnostics, and Windows resume handling.

Rust migration stack:

- `native/razer-core/src/protocol.rs` mirrors the Razer battery protocol.
- `native/razer-core/src/device_db.rs` consumes the same JSON device database as JavaScript.
- `native/razer-core/src/state.rs` provides normalized readings and device states.
- `native/razer-core/src/manager.rs` provides multi-device state management and primary-device selection.
- `native/razer-core/src/events.rs` provides the core event contract.
- `native/razer-core/src/transport.rs` defines the UI-independent battery transport boundary.
- A Windows HID enumeration probe is built with the `hidapi` Windows-native backend.

## Phase 7 hardware validation

The compatibility runtime now has debounced USB attach/detach refreshes, fast retry after transient enumeration failures, delayed refresh after Windows resume, and privacy-safe diagnostics.

Real hardware validation remains necessary for Synapse coexistence and HID collection selection. See `docs/TESTING.md` and `docs/TRANSPORT.md`.

## Phase 8 status

The stable non-hardware-dependent core contracts have now been migrated to Rust. The remaining Phase 8 blocker is the real Windows HID battery transport, which depends on the Phase 7 hardware probe result.

Seelen UI remains an optional integration and must not become a dependency of the Rust core.

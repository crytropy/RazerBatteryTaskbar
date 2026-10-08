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
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | In progress |
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
- `src/transport/webusb-transport.js` owns WebUSB enumeration and USB hotplug monitoring.
- `src/devices/razer-products.json` is the maintainable supported-device database.
- `src/protocol/razer-protocol.js` builds/parses Razer battery protocol messages.
- `src/usb/razer-battery-reader.js` performs Razer control transfers using an injected transport.
- `src/ui/tray-controller.js` renders normalized device state.
- `src/main.js` coordinates lifecycle, refresh scheduling, and Windows resume handling.

The compatibility transport now supports debounced attach/detach refreshes, fast retry after temporary enumeration failures, and a delayed refresh after Windows resumes from sleep. The 30-second poll remains as a safety net.

See `docs/TRANSPORT.md` for the Windows driver and Rust-transport decision policy.

## Next Phase 7 work

Real-hardware verification is required before selecting the final Rust transport. In particular, test supported Razer hardware with Razer Synapse running, USB unplug/replug, receiver reconnect, and Windows sleep/resume. No automatic driver replacement will be introduced.

After those tests, Phase 8 can implement the selected transport in the Rust core without changing the event/UI contracts.

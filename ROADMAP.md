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
| 9 | Build the lightweight Windows tray frontend | Shell compiling |
| 10 | Add persistent settings | Core/persistence complete |
| 11 | Add low/critical battery notifications | Policy complete; OS delivery pending |
| 12 | Add an optional integration API/IPC boundary | Planned |
| 13 | Add optional Seelen UI integration | Planned |
| 14 | Add automated tests and CI | In progress |
| 15 | Package and release a stable v1.0 of the rewritten application | Planned |

## Current architecture

The Electron application remains the usable compatibility runtime while the Rust/native implementation is developed in parallel.

The Rust core contains protocol handling, the shared device database, normalized state, multi-device management, events, transport abstraction, the core service, privacy-safe frontend snapshots, persistent settings, and notification policy.

The native tray shell now lives in `native/razer-tray`. It has its own Windows event loop, system tray icon/menu, refresh scheduling, and settings loading, but deliberately uses a placeholder transport until HID hardware validation is complete.

## Phase 7 hardware validation

Real hardware validation remains necessary for Synapse coexistence and HID collection selection. See `docs/TESTING.md` and `docs/TRANSPORT.md`.

## Native migration rule

The native tray consumes only frontend-safe snapshots and core notification requests. It must not parse Razer protocol data or directly depend on Seelen UI.

Seelen UI remains an optional integration and must not become a dependency of the Rust core or native tray.

# RazerBatteryTaskbar modernization roadmap

The project will be modernized incrementally. Compatibility and a clean core architecture take priority over adding new UI integrations early.

## Architecture rules

1. Core device/protocol logic must not depend on any UI framework.
2. UI integrations must not talk to USB/HID devices directly.
3. The Windows tray must not parse the Razer protocol itself.
4. Optional integrations such as Seelen UI must be removable without affecting the core application.

## Milestones

| Phase | Goal | Status |
| --- | --- | --- |
| 0 | Baseline current behavior, supported devices, and known issues | In progress |
| 1 | Fix type handling, disconnect state, USB cleanup, and polling reliability | In progress |
| 2 | Split the Electron prototype into device/protocol/transport/UI modules | In progress |
| 3 | Introduce a normalized device state model | Planned |
| 4 | Move supported hardware into a maintainable device database | Planned |
| 5 | Add multi-device support | Planned |
| 6 | Add a device event system | Planned |
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | Planned |
| 8 | Move the stable core to Rust | Planned |
| 9 | Build the lightweight Windows tray frontend | Planned |
| 10 | Add persistent settings | Planned |
| 11 | Add low/critical battery notifications | Planned |
| 12 | Add an optional integration API/IPC boundary | Planned |
| 13 | Add optional Seelen UI integration | Planned |
| 14 | Add automated tests and CI | Planned |
| 15 | Package and release a stable v1.0 of the rewritten application | Planned |

## Current refactor scope

The first refactor intentionally keeps the existing Electron runtime and tray behavior. It separates responsibilities and fixes correctness issues before larger features such as multi-device support or a Rust migration are introduced.

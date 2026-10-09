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
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | 00B7 reading and USB reconnect confirmed; event-driven resume refresh added, awaiting validation |
| 8 | Move the stable core to Rust | Core contracts migrated |
| 9 | Build the lightweight Windows tray frontend | Tray + single background core worker; Windows runtime verification ongoing |
| 10 | Add persistent settings | Core/persistence + tray polling/threshold controls + primary preference wiring complete |
| 11 | Add low/critical battery notifications | Policy + Windows adapter + manual test action complete; runtime verification pending |
| 12 | Add an optional integration API/IPC boundary | Read-only loopback API + explicit transport readiness implemented |
| 13 | Add optional Seelen UI integration | Fancy Toolbar adapter scaffold complete |
| 14 | Add automated tests and CI | Rust quality gates + labeled preview artifacts complete |
| 15 | Package and release a stable v1.0 of the rewritten application | Planned |

## Current architecture

The Electron application remains the usable compatibility runtime while the Rust/native implementation is developed in parallel.

The Rust core contains protocol handling, the shared device database, normalized state, multi-device management, events, transport abstraction, the core service, privacy-safe frontend snapshots, a versioned integration JSON contract, persistent settings, and notification policy.

The experimental `windows-hid-00b7-experimental` transport can be explicitly enabled with `--experimental-hid-00b7`. Normal startup remains on the placeholder transport until repeated-read, reconnect, and Synapse coexistence testing is complete.

The native tray shell lives in `native/razer-tray`. It owns the Windows event loop, system tray icon/menu, settings persistence integration, notification threshold controls, Windows notification adapter, per-user startup registration, single-instance protection, and a read-only loopback status API. A single background worker owns the entire Rust core and HID transport, leaving the UI thread free of device I/O. Default startup uses the placeholder transport; the explicit 00B7 experimental flag enables native HID.

## Phase 7 hardware validation

Real hardware validation remains necessary for Synapse coexistence and HID collection selection. See `docs/TESTING.md` and `docs/TRANSPORT.md`.

## Native migration rule

The native tray consumes only frontend-safe snapshots and core notification requests. It must not parse Razer protocol data or directly depend on Seelen UI.

Seelen UI remains an optional integration and must not become a dependency of the Rust core or native tray. The first Fancy Toolbar adapter lives under `integrations/seelen-ui` and consumes only the read-only loopback API.

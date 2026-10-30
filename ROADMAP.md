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
| 7 | Evaluate Windows HID/HIDAPI/libusb transport and add hotplug/reconnect | Complete — native HID feature reports chosen (ADR-0001); hot-plug, sleep/wake and 16.5 h polling verified on hardware; Synapse coexistence out of scope by decision |
| 8 | Move the stable core to Rust | Core contracts migrated |
| 9 | Build the lightweight Windows tray frontend | Complete — tray + background worker + session poll diagnostics; long-run hardware testing done (see Phase 7 hardware validation) |
| 10 | Add persistent settings | Complete — core/persistence + threshold controls + primary preference + startup mode preservation (the poll interval is fixed at 10 s since #24) |
| 11 | Add low/critical battery notifications | Complete |
| 12 | Add an optional integration API/IPC boundary | Read-only loopback API + explicit transport readiness implemented |
| 13 | Add optional Seelen UI integration | Fancy Toolbar adapter scaffold complete |
| 14 | Add automated tests and CI | Rust quality gates + labeled preview artifacts complete |
| 15 | Package and release a stable v1.0 of the rewritten application | Complete — release pipeline verified end to end with `v1.0.0-rc.1` (#10); `v1.0.0` is tagged from this version bump (#11) |

## After v1.0

Loose ends that do not block the v1.0 release:

- **Hibernate recovery (#16).** Confirm across several nightly hibernations that the tray recovers `connected=true` / `transportHealthy=true` within 60 s of waking, then record the result in `docs/TRANSPORT.md`.
- **Cable-only mode of the DeathAdder V3 Pro (`1532:00B6`, follow-up of #18).** The wired/wireless pairing is implemented and charging over the cable is verified with the receiver attached, but a run with the receiver unplugged (so Windows enumerates `00B6` itself) and the wireless ↔ wired hand-over have not been tested on hardware.
- **More hardware.** The native transport targets only the DeathAdder V3 Pro; other devices in the shared database need the same validation before the Electron legacy build can be retired.

## Current architecture

The native tray is the v1.0 release. The Electron application is kept only as a legacy/compatibility build for hardware the native transport does not support yet.

The Rust core contains protocol handling, the shared device database, normalized state, multi-device management, events, transport abstraction, the core service, privacy-safe frontend snapshots, a versioned integration JSON contract, persistent settings, and notification policy.

The Windows HID transport (`windows-hid`) is the default startup path of the native tray (#8). The retired `--experimental-hid-00b7` flag is accepted as a no-op; `--placeholder-transport` is a debug option for runs without hardware. Running alongside Synapse is not supported (ADR-0001); close Synapse before starting the tray.

The native tray shell lives in `native/razer-tray`. It owns the Windows event loop, system tray icon/menu, settings persistence integration, notification threshold controls, Windows notification adapter, per-user startup registration, single-instance protection, and a read-only loopback status API. A single background worker owns the entire Rust core and HID transport, leaving the UI thread free of device I/O. Default startup uses the native Windows HID transport; the placeholder transport is reachable only through the `--placeholder-transport` debug flag.

## Phase 7 hardware validation

Hot-plug, Windows sleep/wake, idle-mouse wake and a 16.5 h polling run were verified on a DeathAdder V3 Pro receiver in October 2026 (issues #4, #5). The final transport choice is recorded in `docs/adr/0001-windows-hid-feature-reports-as-rust-core-transport.md`. Receiver unplug/replug via device-change notifications was verified on 2026-10-30 (#13). Synapse coexistence is out of scope by decision. Hibernate recovery is being confirmed across the maintainer's nightly hibernations (#16). See `docs/TESTING.md` and `docs/TRANSPORT.md`.

## Native migration rule

The native tray consumes only frontend-safe snapshots and core notification requests. It must not parse Razer protocol data or directly depend on Seelen UI.

Seelen UI remains an optional integration and must not become a dependency of the Rust core or native tray. The first Fancy Toolbar adapter lives under `integrations/seelen-ui` and consumes only the read-only loopback API.

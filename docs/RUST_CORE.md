# Rust core architecture

The Rust migration now mirrors the stable JavaScript core contracts while remaining independent from Electron and Seelen UI.

## Modules

- `protocol` — Razer 90-byte request/response protocol.
- `device_db` — loads the existing shared `src/devices/razer-products.json` database at compile time.
- `state` — normalized internal `DeviceReading` and `DeviceState`.
- `events` — UI-independent device state transition events.
- `manager` — multi-device tracking, deterministic ordering, primary-device selection, and event generation.
- `transport` — the transport trait that future HIDAPI/native Windows implementations must satisfy.
- `service` — the single core entry point for refreshes, transport health, failure policy, and state snapshots.
- `diagnostics` — privacy-safe core diagnostics.
- `frontend` — serial-free state objects intended for Windows Tray, Seelen UI, and future local API consumers.

## Frontend privacy boundary

Internal device IDs may include a serial number so identical devices can be tracked correctly. Those internal IDs and serial numbers are deliberately not exposed through `FrontendDeviceState`.

UI/integration code should consume `CoreService::frontend_snapshot()` rather than internal manager state. This keeps future Tray, Seelen, and local API implementations from accidentally leaking serial numbers.

## CoreService behavior

`CoreService<T: BatteryTransport>` owns the transport and device manager. A frontend only needs to request a refresh and consume snapshots/events.

Transport-wide failures preserve the last connected state for the first two failures. On the third consecutive failure, known connected devices transition to disconnected. A later successful scan resets the transport health counter.

## Compatibility rule

The Rust core intentionally preserves the behavior already established by the JavaScript compatibility implementation:

1. Missing devices are retained as disconnected states.
2. Battery and charging changes produce distinct events.
3. One aggregate `DevicesChanged` event follows a batch containing meaningful changes.
4. Primary-device selection prefers a connected device with readable battery state.
5. Device ordering is Mouse → Headset → Dock → Dongle → Unknown.
6. The device database has one source of truth: `src/devices/razer-products.json`.
7. Three consecutive transport-wide failures are required before existing device state is disconnected.

## Remaining hardware-dependent work

The only major core dependency still blocked on physical hardware validation is the real Windows HID battery transport. The existing HID probe remains read-only until the correct HID collection/interface is confirmed on supported hardware with Razer Synapse running.

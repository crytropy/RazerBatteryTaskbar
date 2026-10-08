# Rust core architecture

The Rust migration now mirrors the stable JavaScript core contracts while remaining independent from Electron and Seelen UI.

## Modules

- `protocol` — Razer 90-byte request/response protocol.
- `device_db` — loads the existing shared `src/devices/razer-products.json` database at compile time.
- `state` — normalized `DeviceReading`, `DeviceState`, device IDs, and battery normalization.
- `events` — UI-independent device state transition events.
- `manager` — multi-device tracking, deterministic ordering, primary-device selection, and event generation.
- `transport` — the transport trait that future HIDAPI/native Windows implementations must satisfy.

## Compatibility rule

The Rust core intentionally preserves the behavior already established by the JavaScript compatibility implementation:

1. Missing devices are retained as disconnected states.
2. Battery and charging changes produce distinct events.
3. One aggregate `DevicesChanged` event follows a batch containing meaningful changes.
4. Primary-device selection prefers a connected device with readable battery state.
5. Device ordering is Mouse → Headset → Dock → Dongle → Unknown.
6. The device database has one source of truth: `src/devices/razer-products.json`.

## Remaining hardware-dependent work

The only major core dependency still blocked on physical hardware validation is the real Windows HID battery transport. The existing HID probe remains read-only until the correct HID collection/interface is confirmed on supported hardware with Razer Synapse running.

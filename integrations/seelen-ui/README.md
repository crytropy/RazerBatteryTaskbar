# Seelen UI integration

This folder contains an optional Fancy Toolbar plugin for Seelen UI.

It is an adapter only. Seelen UI does not access Razer USB/HID devices and is not required by the RazerBatteryTaskbar core or native tray.

## Data flow

```text
Razer device
    ↓
RazerBattery Rust core
    ↓
FrontendSnapshot
    ↓
IntegrationSnapshot v1
    ↓
127.0.0.1:27212/v1/status
    ↓
Seelen Fancy Toolbar remoteData
```

## Development load

With Seelen UI running, from the repository root:

```powershell
slu resource load plugin .\integrations\seelen-ui
```

To unload it:

```powershell
slu resource unload plugin .\integrations\seelen-ui
```

The plugin polls the local read-only endpoint every 5 seconds.

## Display behavior

- A connected primary device with battery data: `83%`
- Charging: `83% ⚡`
- Battery unavailable: `Razer ?%`
- Service/device unavailable: `Razer --`
- Native HID transport not ready: `Razer --` with an explicit tooltip explaining that HID is pending

The tooltip lists all connected Razer devices and their battery state. It distinguishes a transport that is not ready from a ready transport that simply has no supported devices connected.

## Current limitation

The native tray's real HID battery transport is still waiting for Phase 7 hardware validation. Until that transport is connected, the native API correctly reports no Razer device rather than fabricating battery data.

## Isolation rule

This integration must remain removable. Nothing under `native/razer-core` or the HID transport may import, call, or depend on Seelen UI.

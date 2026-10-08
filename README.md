# RazerBatteryTaskbar

A Windows tray utility for displaying battery state from supported Razer wireless devices.

This fork is being modernized incrementally. The original Electron/USB application remains the **usable compatibility build**, while a lightweight Rust/native replacement is developed and hardware-validated in parallel.

## Which build should I use?

### Compatibility build — use this for normal testing

The Electron build is currently the version intended to read real device battery data.

Download the latest automatic Windows installer:

https://github.com/crytropy/RazerBatteryTaskbar/releases/tag/dev-build

Asset:

`RazerBatteryTaskbar-Windows-Setup.exe`

The development release is automatically rebuilt from `main` when compatibility-runtime code changes.

### Native Preview — architecture/UI testing only

The Rust/native tray is already able to run as a lightweight Windows tray application and includes:

- native tray icon and menu;
- multi-device core/state model;
- persistent settings;
- runtime polling interval control;
- low/critical battery thresholds;
- Windows battery notifications;
- Start with Windows;
- single-instance protection;
- privacy-safe frontend snapshots;
- read-only local integration API;
- optional Seelen UI adapter.

**The real native HID battery transport is not enabled yet.** The Native Preview deliberately uses a placeholder transport until physical-device HID validation is complete, so it will not report real Razer battery values yet.

The latest preview is available from the **Rust Native Preview** workflow artifacts:

https://github.com/crytropy/RazerBatteryTaskbar/actions/workflows/rust-core.yml

Artifact:

`RazerBatteryTaskbar-Native-Preview`

The artifact contains:

- `RazerBatteryTaskbar-Native-Preview.exe`
- `SHA256SUMS.txt`
- `README-NATIVE-PREVIEW.txt`

### HID Probe — hardware validation tool

The same Rust workflow produces:

`RazerBatteryTaskbar-HID-Probe`

This is a read-only diagnostic utility used to enumerate Razer HID collections before the native battery transport is enabled. It does not send the Razer battery protocol request.

## Architecture

```text
Razer device
    │
    ├─ Electron compatibility runtime ── Windows Tray
    │
    └─ Native transport (hardware validation pending)
             │
             ▼
        Rust Core
             │
      ┌──────┼────────┐
      ▼      ▼        ▼
   Tray   Local API  Future adapters
             │
             ▼
        Seelen UI
```

The architecture follows four rules:

1. Rust Core does not depend on a UI framework.
2. UI integrations do not operate USB/HID devices directly.
3. Windows Tray does not parse the Razer protocol.
4. Optional integrations such as Seelen UI can be removed without affecting the core application.

See [ROADMAP.md](ROADMAP.md) for migration status.

## Local integration API

The Native Preview exposes a read-only loopback endpoint:

```text
http://127.0.0.1:27212/v1/status
```

The versioned JSON contract includes device name/type, product/vendor IDs, battery, charging state, connection state, and transport health.

It deliberately excludes:

- USB serial numbers;
- internal device IDs;
- HID paths;
- interface numbers;
- raw Razer protocol data.

The endpoint is GET-only and does not provide HID or settings-control commands.

See [docs/INTEGRATION.md](docs/INTEGRATION.md).

## Seelen UI

Seelen UI support is an **optional adapter**, not part of the core architecture.

The Fancy Toolbar integration lives in:

```text
integrations/seelen-ui/
```

It reads only the local status API through Seelen Fancy Toolbar `remoteData`.

Development load:

```powershell
slu resource load plugin .\integrations\seelen-ui
```

Unload:

```powershell
slu resource unload plugin .\integrations\seelen-ui
```

See [integrations/seelen-ui/README.md](integrations/seelen-ui/README.md).

## Running the compatibility source

Requirements:

- Windows
- Node.js 18
- npm

```powershell
git clone https://github.com/crytropy/RazerBatteryTaskbar.git
cd RazerBatteryTaskbar
npm ci
npm start
```

Tests:

```powershell
npm test
```

Build the Electron Windows installer:

```powershell
npm run make
```

## Building the Rust workspace

Install the stable Rust toolchain, then:

```powershell
cargo test --workspace
cargo build -p razer-tray --release
```

The native tray executable is produced at:

```text
target/release/razer-tray.exe
```

Build the read-only HID enumeration probe:

```powershell
cargo build -p razer-core --example hid-probe --features windows-hid --release
```

See [docs/RUST_CORE.md](docs/RUST_CORE.md), [docs/NATIVE_TRAY.md](docs/NATIVE_TRAY.md), and [docs/TESTING.md](docs/TESTING.md).

## Device database

Supported/known Razer USB product IDs and protocol transaction IDs have a single shared source of truth:

```text
src/devices/razer-products.json
```

The database currently covers devices/variants from these families:

- Basilisk X / Ultimate / V3 Pro / V3 X HyperSpeed
- DeathAdder V2 / V3 Pro
- Viper Ultimate / V2 Pro
- Naga V2 Pro
- Cobra Pro
- Mouse Dock Pro
- HyperPolling Wireless Dongle
- BlackShark V2 Pro (2020 and 2023 variants)

Actual compatibility still depends on the transport/interface used by a given hardware/firmware combination.

## Credits

The original project and protocol implementation build on work from:

- [OpenRazer](https://github.com/openrazer/openrazer)
- [hsutungyu/razer-mouse-battery-windows](https://github.com/hsutungyu/razer-mouse-battery-windows)

Original project author: Tyler Dougherty.

License: ISC.

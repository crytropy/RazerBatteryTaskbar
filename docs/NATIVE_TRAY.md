# Native Windows tray

The native Windows tray lives in `native/razer-tray`.

It intentionally does not pretend the HID migration is complete. The shell currently uses a placeholder transport and reports:

`Native shell ready — HID transport pending`

## Responsibilities

The native tray owns Windows/UI concerns only:

- Win32-compatible event loop through winit;
- system tray icon and menu through tray-icon;
- periodic refresh scheduling with a runtime-adjustable polling interval;
- tray Settings submenu for polling and notification thresholds;
- frontend-safe core snapshots;
- Windows desktop notification delivery through notify-rust;
- persistent Notifications on/off toggle;
- per-user Start with Windows registration;
- single-instance protection;
- Refresh and Quit actions.

It does not parse Razer packets, enumerate HID collections, or know device serial numbers.

## Tray settings

The Settings submenu contains:

- Notifications;
- Low battery threshold;
- Critical battery threshold;
- Polling interval;
- Start with Windows.

Changing the polling interval updates the active scheduler immediately. It does not create a second polling thread and does not require an application restart.

Threshold changes are validated before persistence. Critical-battery choices above the selected low-battery threshold are disabled.

## Start with Windows

The tray uses the per-user Windows Run registry key:

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`

The value name is `RazerBatteryTaskbar` and the value is the quoted path of the currently running native tray executable.

The registry is treated as the source of truth on startup. If it differs from `settings.json`, the saved setting is reconciled to the actual Windows registration state.

The command is rejected if it exceeds the documented 260-character Run-entry limit.

## Single-instance behavior

The native tray holds a named process mutex for its lifetime. If another copy is launched manually or by an overlapping startup action, the secondary process exits immediately instead of creating a second tray icon or a second polling loop.

## Notification delivery

The Rust core decides when a low or critical battery notification should exist. The tray only translates the resulting `NotificationRequest` into a desktop notification.

Low battery uses normal urgency. Critical battery uses critical urgency. The notification toggle is stored in the same per-user JSON settings file as the other application settings.

Windows toast and startup behavior still require real desktop runtime verification; CI verifies compilation and unit tests without changing the runner's startup registration.

## Next steps

1. Add primary-device selection once real HID devices can be enumerated by the native transport.
2. Replace `PlaceholderTransport` with the real HID transport after hardware validation.
3. Package the native tray separately from the Electron compatibility build.


## CI preview artifacts

The Rust workflow produces two clearly separated Windows artifacts.

`RazerBatteryTaskbar-Native-Preview` contains:

- `RazerBatteryTaskbar-Native-Preview.exe`;
- `SHA256SUMS.txt`;
- `README-NATIVE-PREVIEW.txt`.

The README explicitly states that the real HID battery transport is not enabled yet, so the preview cannot be mistaken for the current compatibility build.

`RazerBatteryTaskbar-HID-Probe` contains the read-only hardware enumeration probe, its SHA-256 checksum, and a diagnostic README.

The Electron `Development Build` remains the usable compatibility build until native HID validation is complete.

# Native Windows tray

The native Windows tray lives in `native/razer-tray`.

It intentionally does not pretend the HID migration is complete. The shell currently uses a placeholder transport and reports:

`Native shell ready — HID transport pending`

## Responsibilities

The native tray owns Windows/UI concerns only:

- Win32-compatible event loop through winit;
- system tray icon and menu through tray-icon;
- periodic refresh scheduling based on persistent settings;
- frontend-safe core snapshots;
- Windows desktop notification delivery through notify-rust;
- persistent Notifications on/off toggle;
- per-user Start with Windows registration;
- Refresh and Quit actions.

It does not parse Razer packets, enumerate HID collections, or know device serial numbers.

## Start with Windows

The tray uses the per-user Windows Run registry key:

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`

The value name is `RazerBatteryTaskbar` and the value is the quoted path of the currently running native tray executable.

The registry is treated as the source of truth on startup. If it differs from `settings.json`, the saved setting is reconciled to the actual Windows registration state.

The command is rejected if it exceeds the documented 260-character Run-entry limit.

## Notification delivery

The Rust core decides when a low or critical battery notification should exist. The tray only translates the resulting `NotificationRequest` into a desktop notification.

Low battery uses normal urgency. Critical battery uses critical urgency. The notification toggle is stored in the same per-user JSON settings file as the other application settings.

Windows toast and startup behavior still require real desktop runtime verification; CI verifies compilation and unit tests without changing the runner's startup registration.

## Next steps

1. Add settings UI/menu actions for thresholds, polling, and primary device.
2. Add single-instance protection for the native tray.
3. Replace `PlaceholderTransport` with the real HID transport after hardware validation.
4. Package the native tray separately from the Electron compatibility build.

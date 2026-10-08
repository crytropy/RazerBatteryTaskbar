# Native Windows tray

Phase 9 has a compiling Windows tray shell in `native/razer-tray`.

The shell intentionally does not pretend the HID migration is complete. It currently uses a placeholder transport that returns no devices, so its status reads:

`Native shell ready — HID transport pending`

## Responsibilities

The native tray owns Windows/UI concerns only:

- Win32-compatible event loop through winit;
- system tray icon and menu through tray-icon;
- periodic refresh scheduling based on persistent settings;
- frontend-safe core snapshots;
- Windows desktop notification delivery through notify-rust;
- persistent Notifications on/off toggle;
- Refresh and Quit actions.

It does not parse Razer packets, enumerate HID collections, or know device serial numbers.

## Notification delivery

The Rust core decides when a low or critical battery notification should exist. The tray only translates the resulting `NotificationRequest` into a desktop notification.

Low battery uses normal urgency. Critical battery uses critical urgency. The notification toggle is stored in the same per-user JSON settings file as the other application settings.

Windows toast behavior still requires real desktop runtime verification; CI only verifies compilation and unit tests.

## Next steps

1. Add settings UI/menu actions for thresholds, polling, and primary device.
2. Add start-with-Windows registration.
3. Replace `PlaceholderTransport` with the real HID transport after hardware validation.
4. Package the native tray separately from the Electron compatibility build.

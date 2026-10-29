# Settings and notification policy

## Persistent settings

The Rust core now defines versioned settings and a persistent JSON store.

The default per-user settings location is resolved with `directories::ProjectDirs`. On Windows this maps to the user's standard roaming application-data location.

Current settings:

- poll interval;
- primary-device preference (`Auto` or a preferred Razer product ID), stored as `"primaryDevice": "auto"` or `"primaryDevice": { "productId": 183 }` (the decimal product ID, `0x00B7` here);
- notifications enabled/disabled;
- low-battery threshold;
- critical-battery threshold;
- start-with-Windows preference.

The default values are:

- poll every 10 seconds (the native tray always uses this value);
- automatic primary-device selection;
- notifications enabled;
- low battery at 20%;
- critical battery at 10%;
- Windows startup disabled.

Settings are validated before save and after load. Missing settings files fall back to defaults rather than being treated as errors.

## Notification rules

Notification evaluation is kept inside the Rust core, while actual Windows toast delivery belongs to the Windows frontend.

A notification request is generated when:

- a device connects and is already below a configured threshold;
- battery state crosses from normal into low;
- battery state crosses from low into critical;
- battery state becomes available for the first time and is already low/critical.

Repeated samples inside the same severity band do not generate another request. Battery recovery, disconnection, and charging-state-only changes do not generate low-battery notifications.

Notification payloads use the serial-free frontend device state, so serial numbers and internal device IDs do not cross the frontend boundary.


## Native tray settings menu

The Windows tray exposes a Settings submenu for the commonly changed values.

The polling interval is not user-configurable: the native tray polls every 10 seconds. One poll opens the HID handle once and spends about 1.2 s on its two queries (ADR-0001), and the charging cable raises no USB event while the receiver is connected, so 10 s is what lets the charging state and icon colour follow the cable within seconds without keeping the handle busy. `pollIntervalSeconds` stays in `settings.json` for the core and other frontends (5–3600 s accepted), but the tray rewrites any other value to 10 at startup.

Low-battery threshold choices are 10%, 15%, 20%, 25%, and 30%.

Critical-battery threshold choices are 5%, 10%, 15%, and 20%. Critical values above the current low-battery threshold are disabled. If the low threshold is lowered below the current critical threshold, the critical threshold is clamped down to the new low threshold so the persisted settings remain valid.

The Settings submenu also contains Notifications and Start with Windows toggles.

## Native tray primary-device menu

The `Primary device` submenu of the right-click menu writes the primary-device preference. `Automatic` stores `"auto"`; choosing a listed device stores its product ID. The change is applied to the running core immediately and persists across restarts. When the preferred product is not connected the core falls back to automatic selection without changing the saved preference, so the device is shown again when it reconnects. The list of devices offered by the menu comes from the latest core snapshot, not from the settings file; see [NATIVE_TRAY.md](NATIVE_TRAY.md).

Values outside the menu presets can still exist in the JSON settings file as long as they satisfy core validation. In that case no preset is checked until a menu preset is selected.

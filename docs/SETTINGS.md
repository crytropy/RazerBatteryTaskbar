# Settings and notification policy

## Persistent settings

The Rust core now defines versioned settings and a persistent JSON store.

The default per-user settings location is resolved with `directories::ProjectDirs`. On Windows this maps to the user's standard roaming application-data location.

Current settings:

- poll interval;
- primary-device preference (`Auto` or a preferred Razer product ID);
- notifications enabled/disabled;
- low-battery threshold;
- critical-battery threshold;
- start-with-Windows preference.

The default values are:

- poll every 30 seconds;
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

Polling interval choices:

- 5 seconds;
- 15 seconds;
- 30 seconds;
- 1 minute;
- 5 minutes.

Changing the polling interval updates the running scheduler immediately and triggers a refresh. Restarting the application is not required.

Low-battery threshold choices are 10%, 15%, 20%, 25%, and 30%.

Critical-battery threshold choices are 5%, 10%, 15%, and 20%. Critical values above the current low-battery threshold are disabled. If the low threshold is lowered below the current critical threshold, the critical threshold is clamped down to the new low threshold so the persisted settings remain valid.

The Settings submenu also contains Notifications and Start with Windows toggles.

Values outside the menu presets can still exist in the JSON settings file as long as they satisfy core validation. In that case no preset is checked until a menu preset is selected.

# Native Windows tray

The native Windows tray lives in `native/razer-tray`.

It intentionally does not pretend the HID migration is complete. The shell currently uses a placeholder transport and reports:

`Native shell ready — HID transport pending`

## Responsibilities

The native tray owns Windows/UI concerns only:

- Win32-compatible event loop through winit;
- system tray icon and menu through tray-icon;
- a dedicated battery-reading worker with runtime-adjustable polling interval;
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
- Send test notification;
- Low battery threshold;
- Critical battery threshold;
- Polling interval;
- Start with Windows.

Changing the polling interval updates the active core worker immediately. It does not start another USB/HID thread and does not require an application restart.

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

`Send test notification` bypasses the battery policy and notification-enabled setting on purpose. It sends one normal Windows notification so toast behavior can be verified without a real HID battery reading or waiting for a low-battery threshold.

Windows toast and startup behavior still require real desktop runtime verification; CI verifies compilation and unit tests without changing the runner's startup registration.

## Next steps

1. Collect HID Probe reports from real Razer hardware while Synapse is running.
2. Validate which HID collections support battery requests without changing drivers.
3. Replace `PlaceholderTransport` with the real HID transport after hardware validation.
4. Add a primary-device selection menu when real devices are available.


## CI preview artifacts

The Rust workflow produces two clearly separated Windows artifacts.

`RazerBatteryTaskbar-Native-Preview` contains:

- `RazerBatteryTaskbar-Native-Preview.exe`;
- `SHA256SUMS.txt`;
- `README-NATIVE-PREVIEW.txt`.

The README explicitly states that the real HID battery transport is not enabled yet, so the preview cannot be mistaken for the current compatibility build.

`RazerBatteryTaskbar-HID-Probe` contains the read-only hardware enumeration probe, its SHA-256 checksum, and a diagnostic README.

The Electron `Development Build` remains the usable compatibility build until native HID validation is complete.

The HID Probe supports `--json` for structured reports, `--pid 0xNNNN` for a
specific model, and `--known-only` to filter by the shared product database.
See [Windows hardware verification](TESTING.md).

## Native background polling architecture

`native/razer-tray/src/polling.rs` now owns a single background `CoreWorker`
which exclusively owns `CoreService` and the selected battery transport.
The GUI thread never calls `CoreService::refresh()` or touches HID.

The worker:

- reads immediately when started and subsequently on its configured interval;
- accepts manual Refresh and interval changes as commands, without creating
  concurrent HID readers;
- coalesces queued Refresh requests before a read;
- sends frontend-safe snapshots and notification requests to the
  `winit` event loop for all tray, toast and local API updates;
- stops when the tray exits, without blocking the UI on a slow HID handle;
- preserves the existing three-failure disconnect policy and recovery.

The icon clears an old battery fill when a transport error is reported,
instead of keeping a stale battery percentage visible. The
`transportHealthy` field indicates read errors independently from
`transportReady` and the device's `connected` state.

The worker is tested with a fake transport, without touching real hardware.
Long-term HID reliability, physical reconnect behavior, and Windows sleep
recovery still require real-device validation.

## Experimental 00B7 native HID (explicit opt-in)

An actual one-shot HID battery query succeeded on a DeathAdder V3 Pro receiver:
`VID=1532 PID=00B7`, `MI_00`, usage page `0001`, usage `0002`.
The Razer reply acknowledged the command, passed transaction and checksum
checks, and returned 78.8%. This validates real device/query functionality. The user also confirmed that
battery reads work with Synapse both open and closed, but long-term
polling stability and reconnect remain under test.

Default startup continues to use `PlaceholderTransport`.

To opt in from PowerShell after quitting Synapse and Razer services:

```powershell
.\RazerBatteryTaskbar-Native-Preview.exe --experimental-hid-00b7
```

Experimental mode uses `windows-hid-00b7-experimental`, attempts only this
specific product/interface/usage combination, and sends a battery query on
launch and at intervals of **at least 60 seconds**. The 5/15/30-second
poll options are disabled while it is active; the minimum is enforced
regardless of earlier saved settings. It opens a handle for a single query
then releases it, and does not change the Windows driver or mouse settings.

It will stop reporting a stale reading when transport health fails,
and will mark the device disconnected after the core's existing three-failure
threshold. The local API reports `transportName`, `transportReady`,
`transportHealthy`, and battery state so that external consumers can
tell experimental reads from the normal placeholder mode.

The original Electron compatibility build remains the recommended
everyday version until reconnect, repeated polling, and Synapse
coexistence pass real-device testing.

## Faster recovery after sleep / hibernation

Windows can send `PBT_APMRESUMEAUTOMATIC` and then
`PBT_APMRESUMESUSPEND` for the same wake. The native tray registers a
Windows suspend/resume callback; it deduplicates resume messages received
within four seconds, then asks the background battery worker to refresh
immediately. A second refresh is scheduled three seconds later, allowing
the USB receiver time to re-enumerate before retrying.

The event callback never opens an HID handle and never blocks the UI event
loop. If power notification registration fails, normal scheduled polling
remains available as a fallback. The existing experimental 60-second
interval remains unchanged for ordinary polling; recovery refreshes are
only sent after an actual Windows resume event.

The user has confirmed successful resume recovery with the earlier polling
version, but reported a delay. This change is intended to shorten that
delay; it still requires real-device testing on Windows.


## HID read errors versus actual receiver disconnection

The experimental Windows transport now classifies problems at the source:
enumeration, collection ambiguity, opening the HID collection, sending the
feature query, receiving the reply, invalid response, configuration, or unknown.

A receiver confirmed by successful HID enumeration is not declared unplugged
simply because its battery transaction fails. Its connectivity is preserved
and its battery/charging fields are cleared immediately; API consumers can
inspect the new `transportErrorKind` field. Successful enumeration with no
matching receiver still disconnects it. The existing three-failure fallback
for unclassified or enumeration errors remains in place.

This separation is particularly useful immediately after Windows wake, when
the receiver's HID collection may be visible before feature reports succeed.
It prevents displaying an old percentage as current or conflating read
availability with physical USB attachment.


## Start with Windows and the experimental HID flag

The tray's **Start with Windows** toggle writes a per-user Windows Run
entry for the current executable. It now preserves the launch mode:

- Starting without arguments registers only the executable path;
  the default placeholder mode remains the default after login.
- Starting with `--experimental-hid-00b7` and explicitly enabling the
  toggle registers the executable path **and the same opt-in flag**.
  The next Windows login can therefore read 00B7 battery data.
- Disabling the toggle removes the registration; changing launch modes
  requires intentionally enabling startup again in the desired mode.

No global experimental-HID setting or automatic mode migration is added.
A copied/moved executable may require the startup toggle to be re-enabled
to update the registered path. The executable should be kept in its
permanent location before enabling startup.


## Monitoring prolonged battery polling

`CoreService` now includes privacy-safe per-process poll counters and
timestamp fields, exposed by `GET /v1/status` in `pollStatistics`.
They record total attempts, successful/failed transport reads, the
most recent successful scan, the most recent scan that returned
battery data, and the most recent read duration. No persistent
telemetry log, third-party network request, serial number, or
HID path is introduced.

After confirming that the experimental 00B7 transport can poll correctly,
use the read-only local status endpoint to watch success/failure counts over
a longer session. The counters reset when the EXE is restarted.

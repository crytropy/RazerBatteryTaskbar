# Native Windows tray

The native Windows tray lives in `native/razer-tray`.

It starts with the native Windows HID battery transport by default (`windows-hid`, see [ADR-0001](adr/0001-windows-hid-feature-reports-as-rust-core-transport.md)); no command-line flag is needed to read real battery data. When no supported receiver is attached, the tray starts normally and reports:

`No supported Razer devices detected`

For debugging without hardware, `--placeholder-transport` starts the tray with a transport that reads nothing and reports `transportReady=false`; the status line then says `Placeholder transport (debug) — no battery data`. The retired `--experimental-hid-00b7` flag is still accepted and is a no-op.

## Responsibilities

The native tray owns Windows/UI concerns only:

- Win32-compatible event loop through winit;
- system tray icon and menu through tray-icon;
- a dedicated battery-reading worker with runtime-adjustable polling interval;
- tray Settings submenu for polling and notification thresholds;
- tray Primary device submenu that picks which connected device the icon shows;
- frontend-safe core snapshots;
- Windows desktop notification delivery through notify-rust;
- persistent Notifications on/off toggle;
- per-user Start with Windows registration;
- single-instance protection;
- Refresh and Quit actions.

It does not parse Razer packets, enumerate HID collections, or know device serial numbers.

## Tray settings

### Primary device

The right-click menu has a `Primary device` submenu directly under the status line. It lists `Automatic` plus one entry per currently connected Razer device, named from the device database; the active choice is checked. With one device attached the submenu still shows that single device. The entries are rebuilt from every core snapshot, so a device that is plugged in appears on the next poll or device-change rescan and an unplugged device disappears.

Picking a device sends the preference to the running worker, which answers with a snapshot built from the devices it already knows; the icon, tooltip and `/v1/status` switch at once and no extra HID handle is opened. The choice is stored as `primaryDevice` in `settings.json` (see [SETTINGS.md](SETTINGS.md)) and applied again at the next start.

The preference is keyed on the Razer product ID. When the preferred device is unplugged the core falls back to its default ordering (Mouse → Headset → Dock → Dongle → Unknown); the submenu keeps the preferred device visible as a disabled, checked `… (disconnected)` entry so the fallback is explicit, and the preference takes over again as soon as the device reconnects. `Automatic` returns to the default ordering.

### Settings

The Settings submenu contains:

- Notifications;
- Send test notification;
- Low battery threshold;
- Critical battery threshold;
- Polling interval;
- Start with Windows.

Changing the polling interval updates the active core worker immediately. It does not start another USB/HID thread and does not require an application restart.

The polling interval menu offers 1 minute and 5 minutes. The 60-second minimum is a property of the Windows HID transport (one handle open/close and a 500 ms reply wait per query, ADR-0001), so shorter values are not offered; a `settings.json` saved by an earlier build with a shorter interval is raised to 60 seconds at startup and saved back.

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

Runtime verification (October 2026, DeathAdder V3 Pro, HID transport enabled with the then-required `--experimental-hid-00b7` flag, thresholds low 20 % / critical 15 %; issue #6): the Windows toast fired once when the battery was already below the critical threshold at startup, did not repeat while the level stayed below the threshold, fired again after charging above the low threshold and discharging back below critical, and did not fire while the idle mouse was in `deviceNotResponding` with its last battery value retained. CI still verifies only compilation and unit tests and does not change the runner's startup registration.

## Next steps

1. Collect HID Probe reports from real Razer hardware while Synapse is running.
2. Validate Synapse coexistence with the default HID transport (known limitation in ADR-0001).
3. Generalise the transport beyond the DeathAdder V3 Pro (further database entries).
4. ~~Add a primary-device selection menu when real devices are available.~~ Done in #9 (`Primary device` submenu); multi-device behaviour still needs a second supported device to verify on hardware.


## Release flow and CI artifacts

### Formal releases

Pushing a tag that starts with `v` (for example `v1.0.0`) runs the **Rust
Native** workflow (`.github/workflows/rust-core.yml`): `cargo fmt --check`,
`clippy -D warnings`, the workspace tests, and a release build of
`razer-tray.exe`. If those pass, the `publish-release` job creates the GitHub
Release for the tag with generated release notes and attaches:

- `RazerBatteryTaskbar-Native-<version>.exe`, where `<version>` is the tag
  without the leading `v`;
- `SHA256SUMS.txt`, the SHA-256 of that EXE;
- `README-NATIVE.txt`, the scope notes for the build (supported receiver,
  Synapse caveat, debug options).

A tag with a SemVer pre-release suffix (`v1.0.0-rc.1`, `v1.0.0-beta.2`) is
published as a pre-release; a plain `v1.0.0` is a normal release.

The **Build Windows** workflow (`build-windows.yml`) also runs for `v*` tags
and attaches the Electron installer `RazerBatteryTaskbar-Windows-Setup.exe` to
the same release. The release body labels it as the legacy / compatibility
build. The two workflows never overwrite each other: both publish through
`scripts/Publish-ReleaseAssets.ps1`, which creates the release only when it
does not exist yet and otherwise uploads (with `--clobber`) just the file names
the calling workflow owns. It does not matter which workflow finishes first.

The native EXE reports the `razer-tray` Cargo version, so bump the crate
versions to match the tag before tagging; the workflow only prints a warning
when they differ. See the "發行流程（Releasing）" section of the [README](../README.md) for the
exact commands.

### Development build and workflow artifacts

Every push to `main` that touches native code also attaches
`RazerBatteryTaskbar-Native-dev.exe`, `SHA256SUMS.txt` and `README-NATIVE.txt`
to the `dev-build` pre-release (the Electron `Development Build`). The
`dev-build` tag itself is moved only by the Electron workflow, so the native
files can be newer than the tag; the commit is printed in `README-NATIVE.txt`.

Pull requests and manual runs upload workflow artifacts only:

- `RazerBatteryTaskbar-Native`: the files listed above, with version `dev`;
- `RazerBatteryTaskbar-HID-Probe`: the read-only hardware enumeration probe,
  its SHA-256 checksum, and a diagnostic README;
- `RazerBatteryTaskbar-Battery-Once-Test`: the standalone one-shot diagnostic
  (see [Windows hardware verification](TESTING.md)).

The Electron build remains available as the legacy / compatibility build for
hardware the native transport does not support yet.

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
instead of keeping a stale battery percentage visible. An unavailable
battery reading now draws a question mark inside the battery outline,
visually distinct from a real 0% reading. This is a tray-only change:
it does not modify HID polling, device connectivity, the read-only API,
or the `transportHealthy` / `transportReady` status fields.

### Charging display

When the primary device reports `charging=true`, the tray draws a small
lightning bolt inside the battery cell. The bolt is inverted against the
fill (dark over the white filled part, white over the empty part) so it
stays legible at 16×16 at every battery level. If the battery level is
unknown while charging, the bolt replaces the question mark and is drawn
slightly dimmed, so "charging, level unknown" is still distinct from
"charging, 0%". When `charging` is `false` or `null` the icon is identical
to the non-charging icon described above.

The tooltip and the status menu item append ` — Charging` to the existing
status string, for example `Razer DeathAdder V3 Pro — 64.0% — Charging`
or `Razer DeathAdder V3 Pro — Battery unavailable — Charging`.
Charging-state changes never trigger low-battery notifications
(see [SETTINGS.md](SETTINGS.md)). The tray never parses the Razer
protocol: it only renders the `charging` field of the frontend snapshot,
which the Windows HID transport fills from a second feature query per poll
(see [TRANSPORT.md](TRANSPORT.md)).

A DeathAdder V3 Pro charging over its cable enumerates as the wired product
ID `00B6`. Since #18 the transport reads that twin as the same device as the
receiver `00B7` (database pairing, see [TRANSPORT.md](TRANSPORT.md)): the
status line keeps the base model name `Razer DeathAdder V3 Pro`, the device
stays connected while the cable goes in or out, and the charging mark appears
in the most common charging situation, the cable. The plug / unplug behaviour
still has to be confirmed on hardware (ticket #18).

The worker is tested with a fake transport, without touching real hardware.
Long-term HID reliability, physical reconnect behavior, and Windows sleep
recovery still require real-device validation.

## Default Windows HID transport (`windows-hid`)

The native tray starts with the Windows HID feature-report transport chosen
in ADR-0001. It currently matches one device model, the DeathAdder V3 Pro,
over its receiver `VID=1532 PID=00B7` or its USB cable `PID=00B6`, on
`MI_00`, usage page `0001`, usage `0002`; both product IDs are shown as the
one device `Razer DeathAdder V3 Pro`. Generalising to other database entries
is later work. Hot-plug, Windows sleep/wake, idle-mouse wake and a 16.5 h
polling run were validated on the receiver in October 2026
(`docs/TRANSPORT.md`); the cable path awaits its hardware check (#18).
Synapse coexistence is unverified.

Start it from PowerShell with no arguments:

```powershell
.\RazerBatteryTaskbar-Native-<version>.exe
```

The transport sends a battery query on launch and then at intervals of
**at least 60 seconds**. It opens a handle once per poll, sends the battery
query followed by the charging-status query on that handle, then releases
it, and does not change the Windows driver or mouse settings. With no
matching receiver attached, reads succeed with an empty device list and the
tray shows `No supported Razer devices detected`.

It stops reporting a stale reading when transport health fails, and marks
the device disconnected after the core's existing three-failure threshold.
The local API reports `transportName=windows-hid`, `transportReady`,
`transportHealthy`, and battery state.

Debug options:

- `--placeholder-transport` starts the tray without any battery transport
  (`transportName=placeholder`, `transportReady=false`). It exists for UI
  and integration testing on machines without a receiver.
- `--experimental-hid-00b7` is the retired preview opt-in. It is accepted and
  ignored; the process prints a notice that the HID transport is already the
  default.

## Faster recovery after sleep / hibernation

Windows can send `PBT_APMRESUMEAUTOMATIC` and then
`PBT_APMRESUMESUSPEND` for the same wake. The native tray registers a
Windows suspend/resume callback; it deduplicates resume messages received
within four seconds, so each wake schedules exactly one set of recovery
refreshes.

Real-device testing showed that the DeathAdder V3 Pro and its receiver need
more than three seconds to re-pair after a wake, so refreshes fired too
early only returned `deviceNotResponding`. Recovery refreshes are therefore
staggered at **+3 s, +10 s and +20 s** after the resume event. The first
refresh that produces a real battery read (a successful transport read with
the primary device connected and reporting a battery level) cancels the
remaining attempts. If all three fail, the tray gives up on recovery and the
ordinary scheduled polling takes over unchanged; the device stays
`connected` and the last known battery level is retained, exactly as for
any other `deviceNotResponding` failure. A second wake that arrives after
the four-second window replaces the pending set rather than adding to it.

The power callback and the timer thread never open an HID handle and never
block the UI event loop; they only post events to it, and the refreshes run
on the background battery worker like every other read. If power
notification registration fails, normal scheduled polling remains available
as a fallback. The configured polling interval (60 s minimum) remains
unchanged for ordinary polling; recovery refreshes are only sent after an
actual Windows resume event.

The scheduler is covered by fake-transport unit tests for both paths
(second attempt succeeds; all attempts fail). The end-to-end timing still
requires real-device testing on Windows.


## USB device-change notifications

Waiting for the next 60-second poll made an unplugged receiver look connected
for up to a minute. The native tray now registers a cfgmgr32 device-interface
notification (`CM_Register_Notification` with `GUID_DEVINTERFACE_HID`,
`native/razer-tray/src/device_events.rs`). Like the suspend/resume callback it
needs no window and is delivered on a system thread.

The callback only looks at the vendor ID in the interface path, so plugging
in an unrelated keyboard does not trigger a rescan. One unplug or replug
produces several interface arrival/removal events; a pure debounce state
machine collapses them into a single refresh request 1.5 s after the last
event, plus one settle refresh 3 s later so the mouse has time to re-pair
with a replugged receiver. The refresh is posted to the `winit` event loop as
`UserEvent::DeviceChange` and from there to the background worker, which
coalesces it with any pending poll. No HID handle is ever opened on the
callback, timer or UI thread.

If registration fails the tray logs the error and normal scheduled polling
remains the fallback. The debounce logic and the worker coalescing are unit
tested without hardware; the ≤ 5 s unplug / replug latency still needs a
real-device run (issue #13).

## HID read errors versus actual receiver disconnection

The Windows HID transport classifies problems at the source:
enumeration, collection ambiguity, opening the HID collection, sending the
feature query, receiving the reply, invalid response, device not responding,
configuration, or unknown.

A receiver confirmed by successful HID enumeration is not declared unplugged
simply because its battery transaction fails. Its connectivity is preserved
and its battery/charging fields are cleared immediately; API consumers can
inspect the new `transportErrorKind` field. Successful enumeration with no
matching receiver still disconnects it. The existing three-failure fallback
for unclassified or enumeration errors remains in place.

### Sleeping devices (`deviceNotResponding`)

A reply whose Razer status byte is busy (`0x01`), failure (`0x03`), or
timeout (`0x04`) is a complete, valid exchange with the receiver in which the
wireless device simply did not answer. On a DeathAdder V3 Pro this happens
every time the mouse goes idle. These replies are classified as
`deviceNotResponding` instead of `invalidResponse`, and the last verified
battery/charging values are retained rather than cleared, so the tray keeps
showing the last known percentage until the mouse wakes and the next poll
succeeds. Every non-success status is also written into the transport error
message as `0xNN` for diagnostics. Status `0x05` (not supported) and any
unrecognized status remain `invalidResponse` and still clear the battery.

This separation is particularly useful immediately after Windows wake, when
the receiver's HID collection may be visible before feature reports succeed.
It prevents displaying an old percentage as current or conflating read
availability with physical USB attachment.


## Start with Windows and legacy flagged entries

The tray's **Start with Windows** toggle writes a per-user Windows Run
entry containing only the quoted path of the current executable. Because
the HID transport is the default, no launch flag is ever written.

Run entries created by earlier previews may still end in
`--experimental-hid-00b7`. Such an entry is recognised as "enabled" on
startup (so the toggle shows the real registration state) and is replaced
with the plain executable path the next time the toggle is switched on.
Until then the flag is harmless: the process accepts it as a no-op.

A copied/moved executable may require the startup toggle to be re-enabled
to update the registered path. The executable should be kept in its
permanent location before enabling startup.


## Monitoring prolonged battery polling

`CoreService` now includes privacy-safe per-process poll counters and
timestamp fields, exposed by `GET /v1/status` in `pollStatistics`.
They record total attempts, successful reads, real transport failures
(`failures`), reads where the device did not respond (`deviceNotResponding`,
usually an idle or sleeping mouse, counted apart from `failures`), the
most recent successful scan, the most recent scan that returned
battery data, and the most recent read duration. No persistent
telemetry log, third-party network request, serial number, or
HID path is introduced.

Use the read-only local status endpoint to watch success/failure counts over
a longer session. The counters reset when the EXE is restarted.
`attempts = successes + failures + deviceNotResponding` always holds.

A 16.5 hour run (2026-10-23 to 2026-10-24, issue #5) recorded 440 attempts and
341 successes. The other 99 reads were all `deviceNotResponding` from the idle
mouse, which the earlier counters reported as `failures`; there were no false
disconnects and sleep/wake recovered automatically. Razer Synapse was not
installed, so coexistence is untested. The full validation table is in
[TRANSPORT.md](TRANSPORT.md).

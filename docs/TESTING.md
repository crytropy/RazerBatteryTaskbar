# Windows hardware verification

Phase 7 requires real Razer hardware testing because CI can validate code and packaging but cannot validate driver coexistence or physical hotplug behavior.

## Recommended test pass

Keep Razer Synapse running for the first pass.

1. Launch the latest Development Build.
2. Confirm each connected supported Razer device appears in the tray menu.
3. Choose **Copy diagnostics** and save the result.
4. Unplug a receiver or wired device. The tray should update without waiting for the 30-second poll.
5. Reconnect it. The device should return after the short hotplug settle delay.
6. Put Windows to sleep and resume. Device state should refresh shortly after resume.
7. Use **Refresh** to verify manual recovery still works.
8. Choose **Copy diagnostics** again after any failure.

Repeat without Synapse only if there is a read/access problem while Synapse is running.

## Important driver rule

Do not replace the Razer driver with Zadig/WinUSB merely to make this test pass. Phase 7 is specifically evaluating a transport that can coexist with the normal Windows/Razer stack.

## Diagnostic privacy

The copied diagnostics include application/runtime versions, transport state, VID/PID, product name, connection state, and battery/charging state. Device serial numbers are intentionally excluded.


## Native HID enumeration probe

The native transport is still disabled. The separate HID Probe can collect the
non-invasive information needed to choose candidate HID collections for later
testing. Download the `RazerBatteryTaskbar-HID-Probe` artifact from the latest
[Rust Native Preview workflow](https://github.com/crytropy/RazerBatteryTaskbar/actions/workflows/rust-core.yml),
extract the ZIP, and open PowerShell in the extracted directory.

```powershell
.\RazerBatteryTaskbar-HID-Probe.exe --help
.\RazerBatteryTaskbar-HID-Probe.exe
.\RazerBatteryTaskbar-HID-Probe.exe --json
.\RazerBatteryTaskbar-HID-Probe.exe --pid 0x00AB
.\RazerBatteryTaskbar-HID-Probe.exe --known-only
```

To save a machine-readable report for comparison:

```powershell
.\RazerBatteryTaskbar-HID-Probe.exe --json |
    Out-File -FilePath .\razer-hid-synapse-on.json -Encoding utf8
```

Run the same command with Synapse closed and/or after unplug/reconnect only
if you need to compare enumeration behavior. Preserve the raw report in each
case, along with the device model, connection method (wired/receiver/Bluetooth),
and whether Synapse was running. Record whether battery read access works in
the separate Electron compatibility build.

The probe enumerates Razer vendor-ID HID collections only; it does **not**
open device handles, request feature reports, change Windows drivers, or
perform a battery read. Its report includes only VID/PID, HID interface,
usage page/usage, and a canonical product name from the device database.
Unknown Razer products are identified as unlisted instead of emitting
device-provided strings. No serial number, HID path or internal device ID
is collected.

A `knownProduct` value of `true` confirms **only** that the product ID exists
in the shared database; it does not prove that the HID battery protocol works.

Treat these observations as interface-discovery data, not final evidence of
compatibility. A later physical-device protocol test is required before any
native feature-report requests are enabled.

## Double-click troubleshooting launcher

The ZIP artifact includes `Run-HID-Probe.cmd` next to the executable.
Extract the entire ZIP before running the launcher. Double-clicking the
`.exe` directly opens a console that can close as soon as the probe exits,
even when enumeration succeeds.

The launcher runs `--json`, keeps the command window open, shows the exit
code, and saves `razer-hid-report.json` and `razer-hid-errors.txt` beside
the executable. Share the **exit code** and error log if the probe fails.
If Windows blocks the process or terminates it before producing any output,
the exit code and Windows Event Viewer may provide additional clues.

The launcher does not request elevation, change drivers, open device
handles or send feature reports.

## DeathAdder V3 Pro receiver: first battery query

The user-provided HID enumeration for VID `1532`, PID `00B7` showed
12 collections. The most promising candidate is `interface 0`,
`usage_page=0001`, `usage=0002` (Generic Desktop Mouse).

This agrees with [OpenMouse's documented DeathAdder V3 Pro receiver
hardware test](https://github.com/OpenMouse-Project/mouse-protocol/blob/main/docs/razer-testing.md):
the separate `MI_00` mouse collection responded to the Razer status
protocol, tested with Synapse and all Razer services quit. The test does
not establish coexistence with Synapse.

The `RazerBatteryTaskbar-Battery-Once-Test` GitHub Actions artifact is an
**experimental, opt-in, separate executable**. It is NOT part of the
ordinary read-only HID enumeration probe or the native tray.

It accepts only `--read-battery-once` and will:

1. Require the exact PID/interface/usage combination `00B7 / MI_00 / 0001:0002`.
2. Refuse to proceed if the matching collection is missing or ambiguous.
3. Open one HID handle, send one Razer battery query using
   SetFeature/GetFeature, validate reply status/transaction/command/checksum,
   and report the resulting battery percentage.
4. Exit without changing DPI, polling rate, device settings, or drivers.

**SetFeature is a device command, even though this query asks only for
battery state.** The safer enumeration-only probe never sends such a
command; this separate diagnostic intentionally does so only after you
explicitly pass the flag. Keep the mouse connected and do the first test
with Synapse and other Razer services completely closed.

```powershell
.\RazerBatteryTaskbar-Battery-Once-Test.exe --help
.\RazerBatteryTaskbar-Battery-Once-Test.exe --read-battery-once
```

Record the displayed battery percentage or exact error. Do not switch
to native continuous polling based solely on one successful query:
retry consistency, disconnect/reconnect, and Synapse coexistence all
still need validation.

## Native Tray validation — 00B7 receiver

A one-shot query on the user's device succeeded and returned `Battery: 78.8%`
with a verified acknowledgement and checksum.

The Rust Native Preview starts with the Windows HID transport by default.
With Synapse and Razer background services closed, launch it with no
arguments:

```powershell
.\RazerBatteryTaskbar-Native-Preview.exe
```

Check the tray battery percentage and the versioned status JSON:

```powershell
Invoke-RestMethod http://127.0.0.1:27212/v1/status |
    ConvertTo-Json -Depth 5
```

Look for `transportName=windows-hid`, `transportReady=true` and
`transportHealthy=true`. Repeat at intervals longer than 60 seconds; then
test disconnection/reconnection. After stable operation with Synapse
closed, test Synapse coexistence separately. If the mouse stops responding
or the transport becomes unhealthy, quit the native tray and return to the
normal compatibility version. Never replace the Windows/Razer driver as a
workaround.

Without a receiver attached, the tray must still start, show
`No supported Razer devices detected`, and report `transportReady=true`
with an empty `devices` list. To exercise the tray UI and the status API on
a machine without any Razer hardware, start it with the debug flag
`--placeholder-transport`; `/v1/status` then reports
`transportName=placeholder` and `transportReady=false`. The retired
`--experimental-hid-00b7` flag is accepted as a no-op and behaves exactly
like a no-argument start.

HID reads run on the background worker thread (see
`docs/NATIVE_TRAY.md`), never on the tray event thread.


## Worker-thread regression tests

Rust CI runs unit tests with a fake transport to ensure that:

- the first device read occurs on a background thread, not the UI caller thread;
- a burst of manual refresh requests is coalesced, not issued in parallel;
- a changed polling interval takes effect without restarting the worker.

The actual DeathAdder V3 Pro has now returned verified battery data through
the Native Tray and JSON status API; the user reports successful reads both
with Synapse enabled and disabled. These are positive point-in-time results,
**not** proof of hours-long stress testing or hot-unplug/wake reliability.

For a reconnect test, keep the native tray running, disconnect the
00B7 receiver, inspect `/v1/status`, reconnect the receiver, and confirm
`connected=true`, `transportHealthy=true` and an up-to-date battery value.
Do not substitute driver changes for an inaccessible HID interface.


## Session-level polling health checks

The Native Preview exposes `pollStatistics` in its read-only local API.
With the native tray running:

```powershell
(Invoke-RestMethod http://127.0.0.1:27212/v1/status).pollStatistics
```

Allow the tray to run normally for a longer session. The counter
`attempts` should increase as refreshes occur, while `failures`
should remain low. `deviceNotResponding` is counted separately: it
increases when the device answered busy / failure / timeout, which is
expected while the mouse idles or sleeps, and it is not a transport
failure. The invariant `attempts = successes + failures +
deviceNotResponding` always holds. A `success` means that transport enumeration and its
request completed, but it can also mean a successful empty-device scan.
`lastBatteryReadUnixMs` is the better indicator that an actual battery
percentage was returned. Report timestamps are UTC milliseconds since
Unix epoch. Telemetry is session-only and excludes serials and HID paths.

Result of the long-run check (issue #5, DeathAdder V3 Pro `1532:00B7`,
`--experimental-hid-00b7`, 2026-10-23 to 2026-10-24, 16.5 hours):
440 attempts, 341 successes and 99 reads counted as `failures`. All 99 were
`deviceNotResponding` from the idle mouse (before this field existed), with
no false disconnects. Sleep and wake recovered automatically. Razer Synapse
was not installed, so coexistence was not tested. See the validation table in
[TRANSPORT.md](TRANSPORT.md) for the full list of scenarios.

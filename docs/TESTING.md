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

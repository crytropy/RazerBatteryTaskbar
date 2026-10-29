# Windows transport strategy

## Current compatibility transport

The compatibility implementation uses node-usb WebUSB for enumeration and control transfers. The transport is isolated behind `src/transport/webusb-transport.js` so it can be replaced without changing the device manager, event contract, tray, notification system, or future integrations.

The transport also listens for node-usb attach/detach events and emits one debounced transport change event for supported Razer hardware. A 30-second polling cycle remains as a fallback.

## Reconnect behavior

- Supported USB attach/detach events request an immediate rescan.
- Windows resume requests a rescan after a short settle delay.
- A transport-wide enumeration failure retries after 2 seconds.
- Existing devices are only marked disconnected after three consecutive enumeration failures.
- A normal successful scan that returns no device still disconnects missing devices immediately.

This distinction avoids turning temporary resume/driver errors into false disconnects.

## Real-hardware validation (October 2026)

Measured on a DeathAdder V3 Pro receiver (`1532:00B7`) with the native tray in
`--experimental-hid-00b7` mode, Razer Synapse not installed, sampling
`/v1/status` every 5 seconds. Details and logs are in issues #4 and #5.

| Scenario | Result | Follow-up |
| --- | --- | --- |
| Receiver unplugged | `connected=false` within one 60 s poll; no false transport errors | #13 (USB detach does not force an immediate rescan) |
| Receiver re-plugged | Battery restored within one poll; once faster, likely via the attach event | #13 |
| Windows sleep → wake | Recovers automatically; the +0 s / +3 s resume refreshes fire before the mouse re-pairs, so the real update waited for the next poll (~64 s) | #12 (stagger resume refreshes) |
| Idle mouse sleep → wake | Battery refreshed within ~13 s; last value retained while asleep (`deviceNotResponding`) | — |
| Windows hibernate → wake | Repeatedly failed, but the tray had been started at logon without the opt-in flag and was in placeholder mode all day, so no battery read was possible regardless of hibernate. Retest after the flag is retired | #16, gated on #8 |
| 16.5 h continuous polling | No false disconnects; battery value present throughout; 22.5 % "failures" were all `deviceNotResponding` from the idle mouse | #14 (count `deviceNotResponding` separately) |
| Synapse coexistence | Not tested: Synapse is not installed on the test machine. Remains a known limitation | — |

## Driver policy

The application must not automatically replace a Razer device driver or instruct users to use Zadig as part of normal installation.

node-usb/libusb can require WinUSB-compatible access on Windows and driver ownership can conflict with the normal vendor driver. The current Electron transport is therefore treated as a compatibility layer rather than the final Windows transport.

Before the Rust migration is finalized, HIDAPI / native Windows HID access should be validated against the supported Razer devices and Razer Synapse. The goal is to preserve the normal Razer driver stack and avoid requiring users to modify device drivers.

## Final transport choice

The Rust core's production Windows transport is native HID feature reports through `hidapi` (windows-native backend), opening the device's HID collection for one query at a time. The decision, the rejected alternatives (libusb / WinUSB), the hardware evidence, and the known limitations (Synapse coexistence unverified, single product ID until #18, 60 s minimum poll) are recorded in [ADR-0001](adr/0001-windows-hid-feature-reports-as-rust-core-transport.md). The Electron node-usb transport remains a legacy compatibility layer only.

Phase 7 is complete and #8 made the HID transport the native tray's default. Remaining items are tracked as follow-ups: #12 / #13 (reconnect latency), #16 (hibernate retest), #18 (wired PID).

## Phase 7 exit criteria

Phase 7 is complete when:

1. USB transport is replaceable behind a stable boundary. ✅
2. Attach/detach and Windows resume trigger fast reconnects. ✅ Verified on hardware (#4); latency improvements tracked in #12 and #13.
3. Temporary enumeration failures do not immediately erase state. ✅ Verified over a 16.5 h run (#5).
4. Real hardware testing confirms behavior with and without Razer Synapse. ⚠️ Without Synapse: verified (#4, #5). With Synapse: not tested, recorded as a known limitation.
5. A final transport choice for the Rust core is documented. ✅ [ADR-0001](adr/0001-windows-hid-feature-reports-as-rust-core-transport.md) (#7).


## Read-only HID discovery probe

The Windows HIDAPI `hid-probe` example is used to determine which HID
collections Windows exposes with the original Razer drivers and Synapse.
It supports `--json`, `--known-only`, `--pid 0xNNNN`, and `--help`.

The `--json` report is versioned (schemaVersion 1), sorted by product ID,
interface, usage page and usage for reproducible comparisons. Its
`knownProduct` flag is database membership, not a claim that a HID collection
supports the battery request. It deliberately never opens a HID device
handle or emits a feature report.

The native transport will only be enabled after identifying usable HID
collections and checking that active polling does not interfere with Synapse.
No driver changes are part of this process.

## Verified candidate for 1532:00B7

User HID enumeration found the stock DeathAdder V3 Pro receiver reporting
`MI_00 / usage_page=0001 / usage=0002`, among 12 HID collections.
An independent OpenMouse hardware test reports successful status reads
over this same collection on a DeathAdder V3 Pro receiver with Synapse
fully closed. Microsoft documents Generic Desktop Mouse top-level
collections as system-exclusive, so this is **not** evidence that
simultaneous Synapse access is safe.

The dedicated `battery-once` example is **not** the production
transport. It explicitly requires one command-line flag, matches only
PID `00B7`, sends a single battery query, and fails closed on ambiguous
candidates or invalid reply frames. This may still be blocked by
Windows HID ownership; drivers will not be replaced to work around it.

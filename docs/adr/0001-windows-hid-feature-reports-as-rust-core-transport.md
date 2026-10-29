---
status: accepted
date: 2026-10-29
---

# Windows HID feature reports are the Rust core's battery transport

The Rust core needed a Windows transport for Razer battery queries that works with the stock Razer driver stack. We chose native Windows HID access through `hidapi` (windows-native backend): enumerate the device's HID collections, open the one Razer exposes for its 90-byte protocol, send the battery request as a feature report, read the reply as a feature report, release the handle. This is the path the preview's `windows-hid-00b7-experimental` profile already used; it is the production transport, reported as `windows-hid`, and the opt-in flag was retired in #8.

We chose it because it is the only candidate that never requires replacing or re-binding a device driver. libusb / WinUSB access (what the Electron compatibility build uses via node-usb) can require WinUSB ownership of the interface, which conflicts with the vendor driver and pushes users toward Zadig. The project's driver policy forbids that, so libusb was rejected for the Rust core even though it works in the Electron prototype.

## Evidence

Validated on a DeathAdder V3 Pro receiver (`1532:00B7`) in October 2026, see issues #4 and #5 and the validation table in `docs/TRANSPORT.md`: hot-plug detection within one poll, automatic recovery after Windows sleep, a 16.5 h run with no false disconnects, and correct retention of the last battery value while the mouse itself sleeps (`deviceNotResponding`).

## Known limitations

- **Synapse coexistence is unverified.** The test machine has no Razer Synapse installed. The chosen collection is a Generic Desktop Mouse top-level collection, which Windows documents as system-exclusive, so sharing it with Synapse is not guaranteed. The transport mitigates this by opening the handle once per poll, for the two feature queries of that poll (battery level, then charging status; about one second in total), and polling no more often than every 10 seconds (60 seconds by default). This is recorded as a known limitation for v1.0, not a release blocker; it becomes a blocker only if a user with Synapse reports conflicts.
- **One device model at a time.** The transport currently matches a single device model (the DeathAdder V3 Pro) and one interface/usage combination. Since #18 the model's wired and wireless product IDs (`00B6` / `00B7`) are read as one logical device through the device database's `pairing` annotation, with the wired twin preferred when both are enumerated; the cable path is still awaiting its hardware check. Generalising to other database entries is later work; neither changes the transport mechanism.
- **Minimum poll interval of 10 s** is a property of this transport (one handle open/close per poll, two queries each waiting 500 ms for the reply, about 1.2 s in total), not of the core. It was 60 s until issue #22: plugging or unplugging the charging cable while the receiver is connected raises no USB device event, so the poll interval is the only bound on how quickly the charging state is noticed. The default stays 60 s; 10 s has not had a multi-hour hardware run yet.

## Considered options

- **libusb / WinUSB control transfers** (node-usb in the Electron build): rejected for the Rust core because of driver-ownership conflicts with the vendor driver; kept only as the legacy Electron compatibility runtime until v1.0 ships, then retired.
- **Native HID feature reports via `hidapi`**: chosen.
- **Windows raw `SetupDi`/`HidD_*` calls without `hidapi`**: not pursued; `hidapi`'s windows-native backend already wraps exactly those calls with no extra driver requirement, and dropping it would only add maintenance.

## Consequences

- The Electron node-usb transport is a legacy compatibility layer. New transport work happens only in the Rust core.
- Since #8 the native tray starts with this transport and reads real battery data with no flag. `PlaceholderTransport` remains only as a debug option (`--placeholder-transport`) for UI and integration testing without hardware.
- The 10 s minimum poll interval is enforced by the tray as its normal minimum (menu options and clamping of saved settings), not by the core.
- Hibernate recovery (#16) and Synapse coexistence remain open validation items but do not reopen this decision.

# Integration contract

RazerBatteryTaskbar exposes a transport-neutral, versioned snapshot for optional integrations.

The contract lives in `razer-core::integration`. It is deliberately separate from HID, the Windows tray, and Seelen UI.

## Schema version 1

Example:

```json
{
  "schemaVersion": 1,
  "service": "RazerBatteryTaskbar",
  "coreVersion": "0.1.0",
  "transportName": "windows-hid",
  "transportReady": true,
  "transportHealthy": true,
  "consecutiveTransportFailures": 0,
  "transportErrorKind": null,
  "pollStatistics": {
    "attempts": 10,
    "successes": 9,
    "failures": 1,
    "deviceNotResponding": 0,
    "lastSuccessfulScanUnixMs": 1791500000000,
    "lastBatteryReadUnixMs": 1791500000000,
    "lastDurationMs": 501
  },
  "primaryDevice": {
    "vendorId": 5426,
    "productId": 171,
    "name": "Razer Basilisk V3 Pro Wireless",
    "deviceType": "mouse",
    "battery": 83.2,
    "charging": null,
    "connected": true
  },
  "devices": []
}
```

The device list uses the same fields as `primaryDevice`.

`transportReady` and `transportHealthy` are intentionally separate:

- `transportReady=false` means the selected battery transport is not available/implemented for use yet;
- `transportReady=true, transportHealthy=false` means a real transport exists but recent device reads are failing;
- `transportName` identifies the active transport implementation without exposing HID paths or other private hardware identifiers. It is informational: consumers should branch on `transportReady` / `transportHealthy`, not on this string. The native tray reports `windows-hid` for the default Windows HID transport and `placeholder` for the `--placeholder-transport` debug run. Preview builds before #8 reported `windows-hid-00b7-experimental`; the value change is not a schema change because the field's type and meaning are unchanged.

## Privacy boundary

The integration contract does not contain:

- USB serial numbers;
- internal device IDs;
- HID paths;
- interface numbers;
- raw Razer protocol data.

Internal identifiers may be needed by the core to distinguish identical hardware, but they never cross this boundary.

## Compatibility policy

Consumers must check `schemaVersion`.

Fields may be added compatibly within a schema version. A breaking rename, semantic change, or field removal requires a new schema version.

## Loopback HTTP transport

The native tray serves the current snapshot at:

`http://127.0.0.1:27212/v1/status`

Properties:

- bound to IPv4 loopback only;
- GET-only;
- read-only;
- `Content-Type: application/json; charset=UTF-8`;
- `Cache-Control: no-store`;
- CORS access limited to Seelen's `http://tauri.localhost` WebView origin;
- no settings mutation;
- no HID commands;
- no serial numbers or internal IDs.

Unknown paths return 404 and non-GET methods return 405. If the port cannot be bound, the tray continues running and only the optional integration endpoint is unavailable.

The endpoint is intended for local consumers such as Seelen Fancy Toolbar `remoteData`, a CLI, or another local adapter.

## Intended transports

The contract does not mandate how it is transported. It can be used by:

- a loopback HTTP endpoint;
- a Windows named pipe;
- a CLI JSON command;
- a Seelen UI adapter;
- future integrations.

The first transport is the read-only loopback HTTP endpoint above. Named pipes remain an option for future privileged or command-oriented integrations, but are unnecessary for the current read-only status contract.


## Plain-text variant for Seelen UI

```text
http://127.0.0.1:27212/v1/status.txt
```

Serves the identical JSON document with `Content-Type: text/plain`. Seelen UI
2.8.x turns `application/json` remote data into a Svelte deep state proxy, and
every read of that proxy from a sandboxed plugin script fails with
`state_unsafe_mutation`, so the bundled Seelen adapter fetches this text
variant and parses it itself. Other integrations should keep using
`/v1/status`.

## Error classification and connectivity semantics

`transportErrorKind` is a stable, privacy-safe reason code (or `null`
after a successful scan). Values are `enumeration`, `open`, `send`,
`receive`, `invalidResponse`, `deviceNotResponding`, `ambiguousCollection`,
`configuration`, and `unknown`.

`transportReady` indicates that a usable transport implementation is selected;
`transportHealthy` describes whether its most recent read completed without an
error. A receiver which was enumerated but could not complete a battery query
remains `connected=true`, with `battery=null` and
`transportHealthy=false`. A successful enumeration that finds no supported
receiver instead reports `connected=false`.

`deviceNotResponding` is the one exception to the `battery=null` rule. It means
the receiver answered with a well-formed Razer reply whose status byte was
busy (`0x01`), failure (`0x03`), or timeout (`0x04`): the wireless device
itself did not answer, which is normal while a mouse is idle/asleep or briefly
out of range. In that case `battery` and `charging` keep their last verified
values and `connected` stays `true`, so integrations should treat them as
"last known" rather than "current". `transportHealthy` is still `false`.
No raw Windows error messages,
serials, or device paths cross this JSON boundary.

The public schema remains version 1 because this new field is additive.


## Polling statistics

The top-level `pollStatistics` object contains session-local read counters,
reset whenever the native tray process starts:

- `attempts`: calls into the selected transport;
- `successes`: transport reads which returned without an error, even if no receiver was present;
- `failures`: transport reads that returned a real error (`enumeration`, `open`,
  `send`, `receive`, `invalidResponse`, `configuration` or `unknown`);
- `deviceNotResponding`: transport reads where the device answered busy / failure /
  timeout, which is what an idle or sleeping wireless mouse does. These are not
  counted in `failures`, so `attempts = successes + failures + deviceNotResponding`;
- `lastSuccessfulScanUnixMs`: timestamp of the most recent successful device scan;
- `lastBatteryReadUnixMs`: timestamp of the most recent successful scan which returned a battery percentage;
- `lastDurationMs`: elapsed time for the most recent attempt, including any HID report response delay.

`deviceNotResponding` was added to `pollStatistics` without changing the
schema version, because the field is additive; `failures` and every other
existing field keep their names, but `failures` no longer includes
`deviceNotResponding` reads.

Times are UTC Unix epoch milliseconds. Timestamps are `null` until that event
occurs. Counts reset on application restart and are not persisted to disk.
The fields help distinguish a receiver that periodically fails to return
battery data from a receiver that has not been seen after sleep.

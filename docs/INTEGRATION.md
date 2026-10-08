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
  "transportHealthy": true,
  "consecutiveTransportFailures": 0,
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

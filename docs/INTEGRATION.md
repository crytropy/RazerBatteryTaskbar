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

## Intended transports

The contract does not mandate how it is transported. It can be used by:

- a loopback HTTP endpoint;
- a Windows named pipe;
- a CLI JSON command;
- a Seelen UI adapter;
- future integrations.

The first planned transport is a read-only loopback endpoint because Seelen Fancy Toolbar supports periodically fetching external JSON through `remoteData`.

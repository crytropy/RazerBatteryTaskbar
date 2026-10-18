// NOTE: Seelen UI evaluates this in @nyariv/sandboxjs, which does not reliably
// short-circuit `&&` / `||` before a property access on a possibly-null value.
// Keep every null check as a plain `if` so no expression touches a property
// of a value that may be null.

// `status` arrives as a plain-text string (from /v1/status.txt), because a
// JSON object remote value becomes a Svelte state proxy that throws
// `state_unsafe_mutation` on any sandboxed read. Parse it here instead.
if (typeof status === "undefined") {
  return "RazerBatteryTaskbar is unavailable or uses an unsupported API version.";
}

let api = null;
if (typeof status === "string") {
  try {
    api = JSON.parse(status);
  } catch (error) {
    api = null;
  }
} else {
  api = status;
}

if (!api) {
  return "RazerBatteryTaskbar is unavailable or uses an unsupported API version.";
}

if (api.schemaVersion !== 1) {
  return "RazerBatteryTaskbar is unavailable or uses an unsupported API version.";
}

if (api.transportReady === false) {
  return "Native Razer HID transport is not ready yet.";
}

const unhealthy = api.transportHealthy === false;

let errorKindText = "";
if (typeof api.transportErrorKind === "string") {
  if (api.transportErrorKind !== "") {
    errorKindText = " (" + api.transportErrorKind + ")";
  }
}

// The receiver answered but the wireless device did not (idle/asleep).
// The native core retains the last verified battery, so it stays displayable.
let asleep = false;
if (unhealthy) {
  if (api.transportErrorKind === "deviceNotResponding") {
    asleep = true;
  }
}

let readFailed = false;
if (unhealthy) {
  if (!asleep) {
    readFailed = true;
  }
}

let readWarning = null;
if (readFailed) {
  readWarning =
    "Battery read temporarily unavailable" +
    errorKindText +
    "; retrying automatically.";
} else if (asleep) {
  readWarning = "Device is idle or asleep; showing last known battery.";
}

const devices = [];
if (Array.isArray(api.devices)) {
  for (const device of api.devices) {
    if (device) {
      if (device.connected) {
        devices.push(device);
      }
    }
  }
}

if (devices.length === 0) {
  if (readWarning) {
    return readWarning;
  }
  return "No supported Razer devices detected.";
}

const lines = [];
for (const device of devices) {
  let name = "Razer device";
  if (device.name) {
    name = device.name;
  }

  const value = device.battery;
  let validBattery = false;
  if (!readFailed) {
    if (typeof value === "number") {
      if (Number.isFinite(value)) {
        if (value >= 0) {
          if (value <= 100) {
            validBattery = true;
          }
        }
      }
    }
  }

  let battery = "Battery unavailable";
  if (validBattery) {
    battery = value.toFixed(1) + "%";
  }

  let charging = "";
  if (validBattery) {
    if (device.charging === true) {
      charging = " · Charging";
    }
  }

  lines.push(name + ": " + battery + charging);
}

const details = lines.join("\n");

if (readWarning) {
  return readWarning + "\n" + details;
}
return details;

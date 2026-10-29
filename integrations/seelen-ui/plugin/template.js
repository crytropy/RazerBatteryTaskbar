// NOTE: Seelen UI evaluates this in @nyariv/sandboxjs, which does not reliably
// short-circuit `&&` / `||` before a property access on a possibly-null value.
// `remoteData` is also absent until the first fetch completes, and a throwing
// template removes the toolbar item entirely. Keep every check as a plain
// nested `if`, and never touch a property of a value that may be null.

// `status` arrives as a plain-text string (from /v1/status.txt), because a
// JSON object remote value becomes a Svelte state proxy that throws
// `state_unsafe_mutation` on any sandboxed read. Parse it here instead.
if (typeof status === "undefined") {
  return "Razer --";
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
  return "Razer --";
}

// Schema 2 only changed what `pollStatistics.failures` counts; every field
// read here has the same meaning in both versions.
let supportedSchema = false;
if (api.schemaVersion === 1) {
  supportedSchema = true;
}
if (api.schemaVersion === 2) {
  supportedSchema = true;
}
if (!supportedSchema) {
  return "Razer --";
}

if (api.transportReady === false) {
  return "Razer --";
}

const device = api.primaryDevice;

if (!device) {
  return "Razer --";
}

if (!device.connected) {
  return "Razer --";
}

// "deviceNotResponding" means the receiver answered but the wireless device
// itself did not (idle/asleep). The native core keeps the last verified
// battery in that case, so it is safe to show it with a sleep marker.
let asleep = false;
if (api.transportHealthy === false) {
  if (api.transportErrorKind === "deviceNotResponding") {
    asleep = true;
  }
}

// An enumerated HID receiver is not the same as a successful battery read.
// Avoid showing stale values while Windows/HID is recovering from sleep.
if (api.transportHealthy === false) {
  if (!asleep) {
    return "Razer ?%";
  }
}

const battery = device.battery;

if (typeof battery !== "number") {
  return "Razer ?%";
}
if (!Number.isFinite(battery)) {
  return "Razer ?%";
}
if (battery < 0) {
  return "Razer ?%";
}
if (battery > 100) {
  return "Razer ?%";
}

// Battery glyph follows Seelen's bundled power plugin thresholds.
let iconName = "PiBatteryWarning";
if (battery > 90) {
  iconName = "PiBatteryFullFill";
} else if (battery > 66) {
  iconName = "PiBatteryHighFill";
} else if (battery > 33) {
  iconName = "PiBatteryMediumFill";
} else if (battery > 5) {
  iconName = "PiBatteryLowFill";
}

// Colour: white at 50% and above, orange below 50%, red below 20%.
let iconColor = "white";
if (battery < 20) {
  iconColor = "#ff4d4f";
} else if (battery < 50) {
  iconColor = "#fa8c16";
}

let charging = "";
if (device.charging === true) {
  charging = " ⚡";
}

let sleeping = "";
if (asleep) {
  sleeping = " 💤";
}

// The Group renders as a <div>; make it a centred flex box so the 1em icon
// sits on the same vertical line as the percentage text next to it.
return [
  Group({
    style: {
      color: iconColor,
      display: "flex",
      alignItems: "center",
      height: "100%",
      lineHeight: 1,
    },
    content: icon(iconName),
  }),
  " ",
  battery.toFixed(0) + "%" + charging + sleeping,
];

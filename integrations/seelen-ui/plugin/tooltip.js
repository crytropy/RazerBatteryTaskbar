if (!status || status.schemaVersion !== 1) {
  return "RazerBatteryTaskbar is unavailable or uses an unsupported API version.";
}

if (status.transportReady === false) {
  return "Native Razer HID transport is not ready yet.";
}

const readFailed = status.transportHealthy === false;
const errorKind =
  typeof status.transportErrorKind === "string" &&
  status.transportErrorKind.length > 0
    ? ` (${status.transportErrorKind})`
    : "";
const readWarning = readFailed
  ? `Battery read temporarily unavailable${errorKind}; retrying automatically.`
  : null;

const devices = Array.isArray(status.devices)
  ? status.devices.filter((device) => device && device.connected)
  : [];

if (devices.length === 0) {
  return readWarning || "No supported Razer devices detected.";
}

const details = devices
  .map((device) => {
    const name = device.name || "Razer device";
    const value = device.battery;
    const validBattery =
      !readFailed &&
      typeof value === "number" &&
      Number.isFinite(value) &&
      value >= 0 &&
      value <= 100;

    const battery = validBattery
      ? `${value.toFixed(1)}%`
      : "Battery unavailable";
    const charging =
      validBattery && device.charging === true ? " · Charging" : "";

    return `${name}: ${battery}${charging}`;
  })
  .join("\n");

return readWarning ? `${readWarning}\n${details}` : details;

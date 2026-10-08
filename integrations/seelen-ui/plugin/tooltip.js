if (!status || status.schemaVersion !== 1) {
  return "RazerBatteryTaskbar is unavailable or uses an unsupported API version.";
}

if (status.transportReady === false) {
  return "Native Razer HID transport is not ready yet.";
}

const devices = Array.isArray(status.devices)
  ? status.devices.filter((device) => device.connected)
  : [];

if (devices.length === 0) {
  return "No supported Razer devices detected.";
}

return devices
  .map((device) => {
    const name = device.name || "Razer device";
    const battery =
      device.battery === null || device.battery === undefined
        ? "Battery unavailable"
        : `${Number(device.battery).toFixed(1)}%`;
    const charging = device.charging === true ? " · Charging" : "";

    return `${name}: ${battery}${charging}`;
  })
  .join("\n");

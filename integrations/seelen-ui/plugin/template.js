if (!status || status.schemaVersion !== 1) {
  return "Razer --";
}

if (status.transportReady === false) {
  return "Razer --";
}

const device = status.primaryDevice;

if (!device || !device.connected) {
  return "Razer --";
}

// An enumerated HID receiver is not the same as a successful battery read.
// Avoid showing stale values while Windows/HID is recovering from sleep.
if (status.transportHealthy === false) {
  return "Razer ?%";
}

const battery = device.battery;

if (
  typeof battery !== "number" ||
  !Number.isFinite(battery) ||
  battery < 0 ||
  battery > 100
) {
  return "Razer ?%";
}

const charging = device.charging === true ? " ⚡" : "";

return `${battery.toFixed(0)}%${charging}`;

if (!status || status.schemaVersion !== 1) {
  return "Razer --";
}

const device = status.primaryDevice;

if (!device || !device.connected) {
  return "Razer --";
}

if (device.battery === null || device.battery === undefined) {
  return "Razer ?%";
}

const battery = Number(device.battery);
const charging = device.charging === true ? " ⚡" : "";

return `${battery.toFixed(0)}%${charging}`;

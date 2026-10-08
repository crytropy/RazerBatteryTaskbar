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

if (device.battery === null || device.battery === undefined) {
  return "Razer ?%";
}

const battery = Number(device.battery);
const charging = device.charging === true ? " ⚡" : "";

return `${battery.toFixed(0)}%${charging}`;

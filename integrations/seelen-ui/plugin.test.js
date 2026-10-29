"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

// Stand-ins for Seelen's component helpers. They flatten to readable strings:
// icon("X") -> "{X}", Group({style:{color}, content}) -> "[color:content]".
function flatten(value) {
  if (Array.isArray(value)) return value.map(flatten).join("");
  return String(value);
}
const helpers = {
  icon: (name) => `{${name}}`,
  Group: (arg) => `[${arg.style.color}:${flatten(arg.content)}]`,
};

// Seelen delivers /v1/status.txt as a string; the scripts JSON.parse it. Objects
// are still accepted so both shapes are exercised.
function render(script, status) {
  const source = fs.readFileSync(path.join(__dirname, "plugin", script), "utf8");
  const value =
    status !== null && typeof status === "object" ? JSON.stringify(status) : status;
  return flatten(
    vm.runInNewContext(`(function () {\n${source}\n})()`, { status: value, ...helpers }, { timeout: 1000 }),
  );
}

function renderObject(script, status) {
  const source = fs.readFileSync(path.join(__dirname, "plugin", script), "utf8");
  return flatten(
    vm.runInNewContext(`(function () {\n${source}\n})()`, { status, ...helpers }, { timeout: 1000 }),
  );
}

function sample(overrides = {}) {
  const primaryDevice = {
    name: "Razer DeathAdder V3 Pro Wireless",
    connected: true,
    battery: 63.92157,
    charging: null,
  };
  return {
    schemaVersion: 2,
    transportReady: true,
    transportHealthy: true,
    transportErrorKind: null,
    primaryDevice,
    devices: [primaryDevice],
    ...overrides,
  };
}

test("shows actual percentage for a healthy connected device", () => {
  assert.equal(render("template.js", sample()), "[white:{PiBatteryMediumFill}] 64%");
  assert.equal(renderObject("template.js", sample()), "[white:{PiBatteryMediumFill}] 64%");
  assert.equal(render("template.js", "{not json"), "Razer --");
  assert.equal(render("template.js", undefined), "Razer --");
  assert.equal(
    render("template.js", sample({
      primaryDevice: { connected: true, battery: 0, charging: true },
    })),
    "[#ff4d4f:{PiBatteryWarning}] 0% ⚡",
  );
});

test("battery icon colour is white >= 50, orange < 50, red < 20", () => {
  const at = (battery) =>
    render("template.js", sample({ primaryDevice: { connected: true, battery, charging: null } }));
  assert.equal(at(95), "[white:{PiBatteryFullFill}] 95%");
  assert.equal(at(70), "[white:{PiBatteryHighFill}] 70%");
  assert.equal(at(50), "[white:{PiBatteryMediumFill}] 50%");
  assert.equal(at(49.9), "[#fa8c16:{PiBatteryMediumFill}] 50%");
  assert.equal(at(25), "[#fa8c16:{PiBatteryLowFill}] 25%");
  assert.equal(at(20), "[#fa8c16:{PiBatteryLowFill}] 20%");
  assert.equal(at(19.9), "[#ff4d4f:{PiBatteryLowFill}] 20%");
  assert.equal(at(3), "[#ff4d4f:{PiBatteryWarning}] 3%");
});

test("does not present a stale percentage during a failed HID transport read", () => {
  const snapshot = sample({
    transportHealthy: false,
    transportErrorKind: "invalidResponse",
  });
  assert.equal(render("template.js", snapshot), "Razer ?%");
  const tooltip = render("tooltip.js", snapshot);
  assert.match(tooltip, /temporarily unavailable/);
  assert.match(tooltip, /invalidResponse/);
  assert.match(tooltip, /retrying automatically/);
  assert.match(tooltip, /Battery unavailable/);
  assert.doesNotMatch(tooltip, /63\.9%/);
});

test("keeps showing the last known percentage while the device is asleep", () => {
  const snapshot = sample({
    transportHealthy: false,
    transportErrorKind: "deviceNotResponding",
  });
  assert.equal(render("template.js", snapshot), "[white:{PiBatteryMediumFill}] 64% 💤");
  const tooltip = render("tooltip.js", snapshot);
  assert.match(tooltip, /idle or asleep/);
  assert.match(tooltip, /last known battery/);
  assert.match(tooltip, /63\.9%/);
  assert.doesNotMatch(tooltip, /Battery unavailable/);

  // Sleeping with no retained value still must not show 0%.
  const noValue = sample({
    transportHealthy: false,
    transportErrorKind: "deviceNotResponding",
    primaryDevice: { connected: true, battery: null, charging: null },
  });
  assert.equal(render("template.js", noValue), "Razer ?%");
});

test("displays unknown when battery is absent or invalid, not zero", () => {
  for (const value of [null, undefined, NaN, Infinity, -1, 101, "64"]) {
    const snapshot = sample({
      primaryDevice: { connected: true, battery: value, charging: false },
    });
    assert.equal(render("template.js", snapshot), "Razer ?%");
  }
  const disconnected = sample({
    primaryDevice: { connected: false, battery: 30, charging: null },
  });
  assert.equal(render("template.js", disconnected), "Razer --");
});

test("shows a no-service marker for missing or unsupported API state", () => {
  for (const snapshot of [null, undefined, sample({ schemaVersion: 3 }), sample({ transportReady: false })]) {
    assert.equal(render("template.js", snapshot), "Razer --");
    assert.match(render("tooltip.js", snapshot), /unavailable|unsupported|not ready/);
  }
});

test("accepts schema versions 1 and 2 alike", () => {
  // Version 2 only changed what `failures` counts; the fields the plugin reads
  // are identical, so a tray still serving version 1 keeps working.
  for (const schemaVersion of [1, 2]) {
    assert.equal(render("template.js", sample({ schemaVersion })), "[white:{PiBatteryMediumFill}] 64%");
    assert.equal(
      render("tooltip.js", sample({ schemaVersion })),
      "Razer DeathAdder V3 Pro Wireless: 63.9%",
    );
  }
});

test("healthy tooltip reports actual battery and charging", () => {
  const device = { name: "Razer Mouse", connected: true, battery: 25.5, charging: true };
  const tooltip = render("tooltip.js", sample({ primaryDevice: device, devices: [device] }));
  assert.equal(tooltip, "Razer Mouse: 25.5% · Charging");
});

test("tooltip explains missing receiver and unavailable service separately", () => {
  assert.equal(
    render("tooltip.js", sample({ devices: [], primaryDevice: null })),
    "No supported Razer devices detected.",
  );
  assert.match(render("tooltip.js", null), /unavailable/);
  assert.match(render("tooltip.js", sample({ transportReady: false })), /not ready/);
});

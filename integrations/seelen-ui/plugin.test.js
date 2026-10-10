"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

function render(script, status) {
  const source = fs.readFileSync(path.join(__dirname, "plugin", script), "utf8");
  return vm.runInNewContext(`(function () {\n${source}\n})()`, { status }, { timeout: 1000 });
}

function sample(overrides = {}) {
  const primaryDevice = {
    name: "Razer DeathAdder V3 Pro Wireless",
    connected: true,
    battery: 63.92157,
    charging: null,
  };
  return {
    schemaVersion: 1,
    transportReady: true,
    transportHealthy: true,
    transportErrorKind: null,
    primaryDevice,
    devices: [primaryDevice],
    ...overrides,
  };
}

test("shows actual percentage for a healthy connected device", () => {
  assert.equal(render("template.js", sample()), "64%");
  assert.equal(
    render("template.js", sample({
      primaryDevice: { connected: true, battery: 0, charging: true },
    })),
    "0% ⚡",
  );
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
  for (const snapshot of [null, undefined, sample({ schemaVersion: 2 }), sample({ transportReady: false })]) {
    assert.equal(render("template.js", snapshot), "Razer --");
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

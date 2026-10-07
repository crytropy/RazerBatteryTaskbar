const test = require('node:test');
const assert = require('node:assert/strict');

const {
    buildBatteryRequest,
    parseBatteryLevel,
} = require('./razer-protocol');

test('buildBatteryRequest creates a 90-byte Razer report', () => {
    const request = buildBatteryRequest(0x1f);

    assert.equal(request.length, 90);
    assert.equal(request[1], 0x1f);
    assert.equal(request[88], 0x85);
    assert.equal(request[89], 0x00);
});

test('parseBatteryLevel converts the response byte to percent', () => {
    const buffer = new ArrayBuffer(90);
    const view = new DataView(buffer);
    view.setUint8(9, 128);

    const battery = parseBatteryLevel(view);

    assert.ok(Math.abs(battery - 50.19607843137255) < 0.000001);
});

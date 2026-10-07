const test = require('node:test');
const assert = require('node:assert/strict');

const {
    DeviceState,
    buildDeviceId,
    normalizeBattery,
} = require('./device-state');

test('normalizes battery values', () => {
    assert.equal(normalizeBattery(-10), 0);
    assert.equal(normalizeBattery(42.5), 42.5);
    assert.equal(normalizeBattery(120), 100);
    assert.equal(normalizeBattery(Number.NaN), null);
});

test('builds a stable USB device id', () => {
    assert.equal(
        buildDeviceId({ vendorId: 0x1532, productId: 0x00ab, serialNumber: 'ABC123' }),
        'usb:1532:00AB:ABC123',
    );
});

test('creates connected and disconnected device states', () => {
    const connected = DeviceState.connected({
        vendorId: 0x1532,
        productId: 0x00ab,
        productName: 'Razer Basilisk V3 Pro Wireless',
        deviceType: 'mouse',
        battery: 83.2,
        serialNumber: 'ABC123',
    });

    assert.equal(connected.connected, true);
    assert.equal(connected.battery, 83.2);
    assert.equal(connected.type, 'mouse');

    const disconnected = DeviceState.disconnected(connected);

    assert.equal(disconnected.connected, false);
    assert.equal(disconnected.battery, null);
    assert.equal(disconnected.id, connected.id);
    assert.equal(disconnected.name, connected.name);
});

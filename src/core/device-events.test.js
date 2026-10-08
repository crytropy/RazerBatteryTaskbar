const test = require('node:test');
const assert = require('node:assert/strict');

const { DEVICE_EVENTS } = require('./device-events');
const { DeviceManager } = require('./device-manager');

function reading(overrides = {}) {
    return {
        vendorId: 0x1532,
        productId: 0x00ab,
        productName: 'Razer Basilisk V3 Pro Wireless',
        deviceType: 'mouse',
        serialNumber: 'MOUSE-1',
        battery: 80,
        charging: null,
        ...overrides,
    };
}

test('emits connection, battery, charging, and disconnection events', () => {
    const manager = new DeviceManager();
    const events = [];

    manager.on(DEVICE_EVENTS.CONNECTED, payload => events.push(['connected', payload]));
    manager.on(DEVICE_EVENTS.BATTERY_CHANGED, payload => events.push(['battery', payload]));
    manager.on(DEVICE_EVENTS.CHARGING_CHANGED, payload => events.push(['charging', payload]));
    manager.on(DEVICE_EVENTS.DISCONNECTED, payload => events.push(['disconnected', payload]));

    manager.updateFromReadings([reading()]);
    manager.updateFromReadings([reading({ battery: 70 })]);
    manager.updateFromReadings([reading({ battery: 70, charging: true })]);
    manager.updateFromReadings([]);

    assert.deepEqual(events.map(([name]) => name), [
        'connected',
        'battery',
        'charging',
        'disconnected',
    ]);

    assert.equal(events[1][1].previous.battery, 80);
    assert.equal(events[1][1].current.battery, 70);
    assert.equal(events[3][1].current.connected, false);
});

test('emits one devices-changed event per update batch with meaningful changes', () => {
    const manager = new DeviceManager();
    const snapshots = [];

    manager.on(DEVICE_EVENTS.DEVICES_CHANGED, payload => snapshots.push(payload));

    manager.updateFromReadings([reading()]);
    manager.updateFromReadings([reading()]);
    manager.updateFromReadings([reading({ battery: 75 })]);

    assert.equal(snapshots.length, 2);
    assert.equal(snapshots[0].devices.length, 1);
    assert.equal(snapshots[1].primaryDevice.battery, 75);
});

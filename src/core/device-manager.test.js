const test = require('node:test');
const assert = require('node:assert/strict');

const { DeviceManager } = require('./device-manager');

function reading(overrides = {}) {
    return {
        vendorId: 0x1532,
        productId: 0x00ab,
        productName: 'Razer Basilisk V3 Pro Wireless',
        deviceType: 'mouse',
        serialNumber: 'MOUSE-1',
        battery: 80,
        ...overrides,
    };
}

test('tracks multiple connected devices', () => {
    const manager = new DeviceManager();

    manager.updateFromReadings([
        reading(),
        reading({
            productId: 0x0555,
            productName: 'Razer Blackshark V2 Pro RZ04-0453',
            deviceType: 'headset',
            serialNumber: 'HEADSET-1',
            battery: 60,
        }),
    ]);

    const devices = manager.getConnectedDevices();

    assert.equal(devices.length, 2);
    assert.equal(devices[0].type, 'mouse');
    assert.equal(devices[1].type, 'headset');
});

test('marks devices missing from the next scan as disconnected', () => {
    const manager = new DeviceManager();

    manager.updateFromReadings([reading()]);
    manager.updateFromReadings([]);

    const [device] = manager.getDevices();

    assert.equal(device.connected, false);
    assert.equal(device.battery, null);
});

test('prefers a connected device with a readable battery as primary', () => {
    const manager = new DeviceManager();

    manager.updateFromReadings([
        reading({ battery: null }),
        reading({
            productId: 0x0555,
            productName: 'Razer Blackshark V2 Pro RZ04-0453',
            deviceType: 'headset',
            serialNumber: 'HEADSET-1',
            battery: 55,
        }),
    ]);

    assert.equal(manager.getPrimaryDevice().battery, 55);
});

const test = require('node:test');
const assert = require('node:assert/strict');

const { buildDiagnostics } = require('./diagnostics');

test('builds privacy-safe diagnostics without serial numbers', () => {
    const text = buildDiagnostics({
        appVersion: '1.0.7',
        platform: 'win32',
        arch: 'x64',
        electronVersion: '22.0.0',
        nodeVersion: '16.17.1',
        transportStatus: {
            name: 'node-usb WebUSB',
            hotplugWatching: true,
        },
        lastScanAt: '2026-10-08T05:00:00.000Z',
        devices: [{
            id: 'usb:1532:00AB:SECRET-SERIAL',
            vendorId: 0x1532,
            productId: 0x00ab,
            name: 'Razer Basilisk V3 Pro Wireless',
            type: 'mouse',
            battery: 83.2,
            charging: null,
            connected: true,
            serialNumber: 'SECRET-SERIAL',
        }],
    });

    assert.match(text, /Razer Basilisk V3 Pro Wireless/);
    assert.match(text, /VID:1532 PID:00AB/);
    assert.match(text, /battery=83\.2%/);
    assert.doesNotMatch(text, /SECRET-SERIAL/);
});

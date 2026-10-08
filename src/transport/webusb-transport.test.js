const test = require('node:test');
const assert = require('node:assert/strict');
const { EventEmitter } = require('events');

const { TRANSPORT_EVENTS } = require('./transport-events');
const {
    WebUsbTransport,
    isSupportedLegacyRazerDevice,
    legacyDeviceIdentity,
} = require('./webusb-transport');

function legacyDevice(vendorId, productId) {
    return {
        deviceDescriptor: {
            idVendor: vendorId,
            idProduct: productId,
        },
    };
}

test('extracts legacy USB identities and filters supported Razer devices', () => {
    const supported = legacyDevice(0x1532, 0x00ab);
    const unsupportedVendor = legacyDevice(0x9999, 0x00ab);

    assert.deepEqual(
        legacyDeviceIdentity(supported),
        { vendorId: 0x1532, productId: 0x00ab },
    );
    assert.equal(isSupportedLegacyRazerDevice(supported), true);
    assert.equal(isSupportedLegacyRazerDevice(unsupportedVendor), false);
});

test('lists devices through the injected WebUSB implementation', async () => {
    const expected = [{ vendorId: 0x1532, productId: 0x00ab }];
    const transport = new WebUsbTransport({
        usbApi: new EventEmitter(),
        webUsb: {
            getDevices: async () => expected,
        },
    });

    assert.equal(await transport.listDevices(), expected);
    transport.dispose();
});

test('debounces supported attach/detach events into one refresh signal', async () => {
    class FakeUsbApi extends EventEmitter {
        unrefHotplugEvents() {}
    }

    const usbApi = new FakeUsbApi();
    const transport = new WebUsbTransport({
        usbApi,
        webUsb: { getDevices: async () => [] },
        hotplugDebounceMs: 10,
    });

    const events = [];
    transport.on(TRANSPORT_EVENTS.DEVICES_CHANGED, event => events.push(event));
    transport.startWatching();

    usbApi.emit('attach', legacyDevice(0x1532, 0x00ab));
    usbApi.emit('detach', legacyDevice(0x1532, 0x00ab));

    await new Promise(resolve => setTimeout(resolve, 30));

    assert.equal(events.length, 1);
    assert.equal(events[0].action, 'detach');
    assert.equal(events[0].vendorId, 0x1532);
    assert.equal(events[0].productId, 0x00ab);

    transport.dispose();
});

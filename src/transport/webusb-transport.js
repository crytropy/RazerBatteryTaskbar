const { EventEmitter } = require('events');
const { WebUSB, usb } = require('usb');
const { isSupportedRazerDevice } = require('../devices/razer-products');
const { TRANSPORT_EVENTS } = require('./transport-events');

const DEFAULT_HOTPLUG_DEBOUNCE_MS = 350;

function legacyDeviceIdentity(device) {
    const descriptor = device?.deviceDescriptor;

    if (!descriptor) {
        return null;
    }

    return {
        vendorId: descriptor.idVendor,
        productId: descriptor.idProduct,
    };
}

function isSupportedLegacyRazerDevice(device) {
    const identity = legacyDeviceIdentity(device);
    return identity ? isSupportedRazerDevice(identity) : false;
}

class WebUsbTransport extends EventEmitter {
    constructor({
        usbApi = usb,
        webUsb = new WebUSB({ allowAllDevices: true }),
        hotplugDebounceMs = DEFAULT_HOTPLUG_DEBOUNCE_MS,
    } = {}) {
        super();

        this.usbApi = usbApi;
        this.webUsb = webUsb;
        this.hotplugDebounceMs = hotplugDebounceMs;
        this.hotplugTimer = null;
        this.watching = false;

        this.handleAttach = device => this.handleHotplug('attach', device);
        this.handleDetach = device => this.handleHotplug('detach', device);
    }

    async listDevices() {
        return this.webUsb.getDevices();
    }

    startWatching() {
        if (this.watching) {
            return;
        }

        this.watching = true;
        this.usbApi.on('attach', this.handleAttach);
        this.usbApi.on('detach', this.handleDetach);

        if (typeof this.usbApi.unrefHotplugEvents === 'function') {
            this.usbApi.unrefHotplugEvents();
        }
    }

    handleHotplug(action, device) {
        if (!isSupportedLegacyRazerDevice(device)) {
            return;
        }

        if (this.hotplugTimer) {
            clearTimeout(this.hotplugTimer);
        }

        const identity = legacyDeviceIdentity(device);

        this.hotplugTimer = setTimeout(() => {
            this.hotplugTimer = null;
            this.emit(
                TRANSPORT_EVENTS.DEVICES_CHANGED,
                Object.freeze({
                    reason: 'hotplug',
                    action,
                    ...identity,
                }),
            );
        }, this.hotplugDebounceMs);
    }

    dispose() {
        if (this.hotplugTimer) {
            clearTimeout(this.hotplugTimer);
            this.hotplugTimer = null;
        }

        if (this.watching) {
            this.usbApi.removeListener('attach', this.handleAttach);
            this.usbApi.removeListener('detach', this.handleDetach);
            this.watching = false;
        }

        this.removeAllListeners();
    }
}

module.exports = {
    WebUsbTransport,
    isSupportedLegacyRazerDevice,
    legacyDeviceIdentity,
};

const { WebUSB } = require('usb');
const {
    RAZER_VENDOR_ID,
    getRazerProduct,
    isSupportedRazerDevice,
} = require('../devices/razer-products');
const {
    BATTERY_REPLY_SIZE,
    buildBatteryRequest,
    parseBatteryLevel,
} = require('../protocol/razer-protocol');

const BATTERY_RESPONSE_DELAY_MS = 500;

function delay(milliseconds) {
    return new Promise(resolve => setTimeout(resolve, milliseconds));
}

class RazerBatteryReader {
    constructor() {
        this.activeDevice = null;
        this.webUsb = new WebUSB({
            devicesFound: devices => devices.find(isSupportedRazerDevice),
        });
    }

    async findSupportedDevice() {
        const device = await this.webUsb.requestDevice({
            filters: [{ vendorId: RAZER_VENDOR_ID }],
        });

        if (!device || !isSupportedRazerDevice(device)) {
            throw new Error('No supported Razer device found on system');
        }

        const product = getRazerProduct(device.productId);
        return { device, product };
    }

    async readBattery() {
        const { device, product } = await this.findSupportedDevice();
        this.activeDevice = device;

        let interfaceNumber = null;
        let interfaceClaimed = false;

        try {
            await device.open();

            if (device.configuration === null) {
                await device.selectConfiguration(1);
            }

            const usbInterface = device.configuration.interfaces[0];
            if (!usbInterface) {
                throw new Error('Razer device has no available USB interface');
            }

            interfaceNumber = usbInterface.interfaceNumber;
            await device.claimInterface(interfaceNumber);
            interfaceClaimed = true;

            const request = buildBatteryRequest(product.transactionId);
            const writeResult = await device.controlTransferOut({
                requestType: 'class',
                recipient: 'interface',
                request: 0x09,
                value: 0x300,
                index: 0x00,
            }, request);

            if (writeResult.status !== 'ok') {
                throw new Error('Battery request failed with status: ' + writeResult.status);
            }

            await delay(BATTERY_RESPONSE_DELAY_MS);

            const reply = await device.controlTransferIn({
                requestType: 'class',
                recipient: 'interface',
                request: 0x01,
                value: 0x300,
                index: 0x00,
            }, BATTERY_REPLY_SIZE);

            if (reply.status !== 'ok' || !reply.data) {
                throw new Error('Battery response failed with status: ' + reply.status);
            }

            return {
                vendorId: device.vendorId,
                productId: device.productId,
                productName: product.name,
                deviceType: product.type,
                serialNumber: device.serialNumber || null,
                battery: parseBatteryLevel(reply.data),
                charging: null,
            };
        } finally {
            await this.releaseDevice(device, interfaceNumber, interfaceClaimed);

            if (this.activeDevice === device) {
                this.activeDevice = null;
            }
        }
    }

    async releaseDevice(device, interfaceNumber, interfaceClaimed) {
        if (interfaceClaimed && device.opened && interfaceNumber !== null) {
            try {
                await device.releaseInterface(interfaceNumber);
            } catch (error) {
                console.warn('[usb] Failed to release interface:', error);
            }
        }

        if (device.opened) {
            try {
                await device.close();
            } catch (error) {
                console.warn('[usb] Failed to close device:', error);
            }
        }
    }

    async dispose() {
        if (!this.activeDevice || !this.activeDevice.opened) {
            return;
        }

        try {
            await this.activeDevice.close();
        } finally {
            this.activeDevice = null;
        }
    }
}

module.exports = {
    RazerBatteryReader,
};

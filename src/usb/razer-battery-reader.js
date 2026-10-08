const { WebUSB } = require('usb');
const {
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

function buildReading(device, product, battery = null) {
    return {
        vendorId: device.vendorId,
        productId: device.productId,
        productName: product.name,
        deviceType: product.type,
        serialNumber: device.serialNumber || null,
        battery,
        charging: null,
    };
}

class RazerBatteryReader {
    constructor() {
        this.activeDevices = new Set();
        this.webUsb = new WebUSB({
            allowAllDevices: true,
        });
    }

    async listSupportedDevices() {
        const devices = await this.webUsb.getDevices();
        return devices.filter(isSupportedRazerDevice);
    }

    async readAllBatteries() {
        const devices = await this.listSupportedDevices();
        const readings = [];

        for (const device of devices) {
            const product = getRazerProduct(device.productId);

            try {
                readings.push(await this.readBatteryFromDevice(device, product));
            } catch (error) {
                console.warn(
                    '[usb] Failed to read ' + product.name + ':',
                    error,
                );

                readings.push(buildReading(device, product));
            }
        }

        return readings;
    }

    async readBattery() {
        const readings = await this.readAllBatteries();

        if (readings.length === 0) {
            throw new Error('No supported Razer device found on system');
        }

        return readings[0];
    }

    async readBatteryFromDevice(device, product) {
        this.activeDevices.add(device);

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

            return buildReading(device, product, parseBatteryLevel(reply.data));
        } finally {
            await this.releaseDevice(device, interfaceNumber, interfaceClaimed);
            this.activeDevices.delete(device);
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
        const activeDevices = Array.from(this.activeDevices);

        for (const device of activeDevices) {
            if (!device.opened) {
                this.activeDevices.delete(device);
                continue;
            }

            try {
                await device.close();
            } catch (error) {
                console.warn('[usb] Failed to close active device:', error);
            } finally {
                this.activeDevices.delete(device);
            }
        }
    }
}

module.exports = {
    RazerBatteryReader,
    buildReading,
};

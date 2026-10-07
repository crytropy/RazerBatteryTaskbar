function normalizeBattery(value) {
    if (!Number.isFinite(value)) {
        return null;
    }

    return Math.min(100, Math.max(0, value));
}

function toHex(value, width = 4) {
    return value.toString(16).toUpperCase().padStart(width, '0');
}

function buildDeviceId({ vendorId, productId, serialNumber }) {
    const baseId = `usb:${toHex(vendorId)}:${toHex(productId)}`;
    return serialNumber ? `${baseId}:${serialNumber}` : baseId;
}

class DeviceState {
    constructor({
        id = null,
        vendorId = null,
        productId = null,
        name = null,
        type = 'unknown',
        battery = null,
        charging = null,
        connected = false,
        serialNumber = null,
        lastUpdated = new Date().toISOString(),
    } = {}) {
        this.id = id;
        this.vendorId = vendorId;
        this.productId = productId;
        this.name = name;
        this.type = type;
        this.battery = normalizeBattery(battery);
        this.charging = charging;
        this.connected = Boolean(connected);
        this.serialNumber = serialNumber;
        this.lastUpdated = lastUpdated;

        Object.freeze(this);
    }

    static connected(reading) {
        if (!reading || !Number.isInteger(reading.vendorId) || !Number.isInteger(reading.productId)) {
            throw new TypeError('A connected device state requires vendorId and productId');
        }

        return new DeviceState({
            id: buildDeviceId(reading),
            vendorId: reading.vendorId,
            productId: reading.productId,
            name: reading.productName ?? null,
            type: reading.deviceType ?? 'unknown',
            battery: reading.battery,
            charging: reading.charging ?? null,
            connected: true,
            serialNumber: reading.serialNumber ?? null,
        });
    }

    static disconnected(previousState = null) {
        if (!(previousState instanceof DeviceState)) {
            return new DeviceState();
        }

        return new DeviceState({
            id: previousState.id,
            vendorId: previousState.vendorId,
            productId: previousState.productId,
            name: previousState.name,
            type: previousState.type,
            battery: null,
            charging: null,
            connected: false,
            serialNumber: previousState.serialNumber,
        });
    }
}

module.exports = {
    DeviceState,
    buildDeviceId,
    normalizeBattery,
};

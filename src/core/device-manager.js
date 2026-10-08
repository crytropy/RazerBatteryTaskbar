const { DeviceState } = require('./device-state');

const DEVICE_TYPE_PRIORITY = Object.freeze({
    mouse: 0,
    headset: 1,
    dock: 2,
    dongle: 3,
    unknown: 4,
});

function compareDevices(left, right) {
    if (left.connected !== right.connected) {
        return left.connected ? -1 : 1;
    }

    const leftPriority = DEVICE_TYPE_PRIORITY[left.type] ?? DEVICE_TYPE_PRIORITY.unknown;
    const rightPriority = DEVICE_TYPE_PRIORITY[right.type] ?? DEVICE_TYPE_PRIORITY.unknown;

    if (leftPriority !== rightPriority) {
        return leftPriority - rightPriority;
    }

    return (left.name ?? left.id ?? '').localeCompare(right.name ?? right.id ?? '');
}

class DeviceManager {
    constructor() {
        this.devices = new Map();
    }

    updateFromReadings(readings = []) {
        if (!Array.isArray(readings)) {
            throw new TypeError('readings must be an array');
        }

        const seenDeviceIds = new Set();

        for (const reading of readings) {
            const nextState = DeviceState.connected(reading);
            seenDeviceIds.add(nextState.id);
            this.devices.set(nextState.id, nextState);
        }

        for (const [deviceId, state] of this.devices.entries()) {
            if (!seenDeviceIds.has(deviceId) && state.connected) {
                this.devices.set(deviceId, DeviceState.disconnected(state));
            }
        }

        return this.getDevices();
    }

    markAllDisconnected() {
        for (const [deviceId, state] of this.devices.entries()) {
            if (state.connected) {
                this.devices.set(deviceId, DeviceState.disconnected(state));
            }
        }

        return this.getDevices();
    }

    getDevices() {
        return Array.from(this.devices.values()).sort(compareDevices);
    }

    getConnectedDevices() {
        return this.getDevices().filter(device => device.connected);
    }

    getPrimaryDevice() {
        const connectedDevices = this.getConnectedDevices();

        return connectedDevices.find(device => Number.isFinite(device.battery))
            ?? connectedDevices[0]
            ?? null;
    }

    clear() {
        this.devices.clear();
    }
}

module.exports = {
    DeviceManager,
    compareDevices,
};

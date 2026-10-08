const { EventEmitter } = require('events');
const { DEVICE_EVENTS } = require('./device-events');
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

function createEventPayload(previous, current) {
    return Object.freeze({
        deviceId: current?.id ?? previous?.id ?? null,
        previous: previous ?? null,
        current: current ?? null,
    });
}

class DeviceManager extends EventEmitter {
    constructor() {
        super();
        this.devices = new Map();
    }

    updateFromReadings(readings = []) {
        if (!Array.isArray(readings)) {
            throw new TypeError('readings must be an array');
        }

        const seenDeviceIds = new Set();
        let changed = false;

        for (const reading of readings) {
            const nextState = DeviceState.connected(reading);
            const previousState = this.devices.get(nextState.id) ?? null;

            seenDeviceIds.add(nextState.id);
            this.devices.set(nextState.id, nextState);

            if (this.emitStateChanges(previousState, nextState)) {
                changed = true;
            }
        }

        for (const [deviceId, state] of this.devices.entries()) {
            if (!seenDeviceIds.has(deviceId) && state.connected) {
                const nextState = DeviceState.disconnected(state);
                this.devices.set(deviceId, nextState);
                this.emit(DEVICE_EVENTS.DISCONNECTED, createEventPayload(state, nextState));
                changed = true;
            }
        }

        if (changed) {
            this.emitDevicesChanged();
        }

        return this.getDevices();
    }

    markAllDisconnected() {
        let changed = false;

        for (const [deviceId, state] of this.devices.entries()) {
            if (!state.connected) {
                continue;
            }

            const nextState = DeviceState.disconnected(state);
            this.devices.set(deviceId, nextState);
            this.emit(DEVICE_EVENTS.DISCONNECTED, createEventPayload(state, nextState));
            changed = true;
        }

        if (changed) {
            this.emitDevicesChanged();
        }

        return this.getDevices();
    }

    emitStateChanges(previousState, nextState) {
        if (!previousState || !previousState.connected) {
            this.emit(
                DEVICE_EVENTS.CONNECTED,
                createEventPayload(previousState, nextState),
            );
            return true;
        }

        let changed = false;

        if (previousState.battery !== nextState.battery) {
            this.emit(
                DEVICE_EVENTS.BATTERY_CHANGED,
                createEventPayload(previousState, nextState),
            );
            changed = true;
        }

        if (previousState.charging !== nextState.charging) {
            this.emit(
                DEVICE_EVENTS.CHARGING_CHANGED,
                createEventPayload(previousState, nextState),
            );
            changed = true;
        }

        return changed;
    }

    emitDevicesChanged() {
        this.emit(
            DEVICE_EVENTS.DEVICES_CHANGED,
            Object.freeze({
                devices: this.getDevices(),
                primaryDevice: this.getPrimaryDevice(),
            }),
        );
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

const { app } = require('electron');
const { DeviceState } = require('./core/device-state');
const { RazerBatteryReader } = require('./usb/razer-battery-reader');
const { TrayController } = require('./ui/tray-controller');

if (require('electron-squirrel-startup')) {
    app.quit();
}

const POLL_INTERVAL_MS = 30_000;

let batteryReader;
let trayController;
let pollTimer = null;
let shuttingDown = false;
let currentState = DeviceState.disconnected();

async function updateBatteryStatus() {
    if (shuttingDown) {
        return;
    }

    try {
        const reading = await batteryReader.readBattery();
        currentState = DeviceState.connected(reading);
    } catch (error) {
        console.error('[battery] Failed to read battery state:', error);
        currentState = DeviceState.disconnected(currentState);
    }

    trayController.setDeviceState(currentState);

    if (!shuttingDown) {
        pollTimer = setTimeout(updateBatteryStatus, POLL_INTERVAL_MS);
    }
}

async function quitApplication() {
    if (shuttingDown) {
        return;
    }

    shuttingDown = true;

    if (pollTimer) {
        clearTimeout(pollTimer);
        pollTimer = null;
    }

    try {
        await batteryReader?.dispose();
    } catch (error) {
        console.warn('[usb] Failed to dispose battery reader:', error);
    }

    trayController?.destroy();
    app.quit();
}

app.whenReady().then(() => {
    batteryReader = new RazerBatteryReader();
    trayController = new TrayController({
        rootPath: app.getAppPath(),
        onQuit: quitApplication,
    });

    trayController.initialize();
    trayController.setDeviceState(currentState);
    updateBatteryStatus();
});

const { app } = require('electron');
const { DEVICE_EVENTS } = require('./core/device-events');
const { DeviceManager } = require('./core/device-manager');
const { RazerBatteryReader } = require('./usb/razer-battery-reader');
const { TrayController } = require('./ui/tray-controller');

if (require('electron-squirrel-startup')) {
    app.quit();
}

const POLL_INTERVAL_MS = 30_000;

let batteryReader;
let deviceManager;
let trayController;
let pollTimer = null;
let pollInProgress = false;
let shuttingDown = false;

function renderDeviceStates() {
    trayController.setDevices(
        deviceManager.getDevices(),
        deviceManager.getPrimaryDevice(),
    );
}

function scheduleNextPoll() {
    if (shuttingDown) {
        return;
    }

    pollTimer = setTimeout(refreshDeviceStates, POLL_INTERVAL_MS);
}

async function refreshDeviceStates() {
    if (shuttingDown || pollInProgress) {
        return;
    }

    pollInProgress = true;

    if (pollTimer) {
        clearTimeout(pollTimer);
        pollTimer = null;
    }

    try {
        const readings = await batteryReader.readAllBatteries();
        deviceManager.updateFromReadings(readings);
    } catch (error) {
        console.error('[battery] Failed to enumerate Razer devices:', error);
        deviceManager.markAllDisconnected();
    } finally {
        pollInProgress = false;
        scheduleNextPoll();
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

    deviceManager?.removeAllListeners();
    trayController?.destroy();
    app.quit();
}

app.whenReady().then(() => {
    batteryReader = new RazerBatteryReader();
    deviceManager = new DeviceManager();
    trayController = new TrayController({
        rootPath: app.getAppPath(),
        onRefresh: refreshDeviceStates,
        onQuit: quitApplication,
    });

    deviceManager.on(DEVICE_EVENTS.DEVICES_CHANGED, renderDeviceStates);

    trayController.initialize();
    renderDeviceStates();
    refreshDeviceStates();
});

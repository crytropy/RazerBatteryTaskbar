const { app } = require('electron');
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

async function updateBatteryStatus() {
    if (shuttingDown) {
        return;
    }

    try {
        const state = await batteryReader.readBattery();
        trayController.setBatteryState(state);
    } catch (error) {
        console.error('[battery] Failed to read battery state:', error);
        trayController.setDisconnected();
    } finally {
        if (!shuttingDown) {
            pollTimer = setTimeout(updateBatteryStatus, POLL_INTERVAL_MS);
        }
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
    updateBatteryStatus();
});

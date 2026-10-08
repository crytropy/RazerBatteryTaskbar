const {
    app,
    powerMonitor,
} = require('electron');
const { DEVICE_EVENTS } = require('./core/device-events');
const { DeviceManager } = require('./core/device-manager');
const { TRANSPORT_EVENTS } = require('./transport/transport-events');
const { WebUsbTransport } = require('./transport/webusb-transport');
const { RazerBatteryReader } = require('./usb/razer-battery-reader');
const { TrayController } = require('./ui/tray-controller');

if (require('electron-squirrel-startup')) {
    app.quit();
}

const POLL_INTERVAL_MS = 30_000;
const RETRY_INTERVAL_MS = 2_000;
const RESUME_REFRESH_DELAY_MS = 1_000;
const MAX_ENUMERATION_FAILURES_BEFORE_DISCONNECT = 3;

let transport;
let batteryReader;
let deviceManager;
let trayController;
let pollTimer = null;
let resumeTimer = null;
let pollInProgress = false;
let refreshPending = false;
let shuttingDown = false;
let enumerationFailureCount = 0;

function renderDeviceStates() {
    trayController.setDevices(
        deviceManager.getDevices(),
        deviceManager.getPrimaryDevice(),
    );
}

function clearPollTimer() {
    if (pollTimer) {
        clearTimeout(pollTimer);
        pollTimer = null;
    }
}

function schedulePoll(delay = POLL_INTERVAL_MS) {
    if (shuttingDown) {
        return;
    }

    clearPollTimer();
    pollTimer = setTimeout(refreshDeviceStates, delay);
}

function requestImmediateRefresh() {
    if (shuttingDown) {
        return;
    }

    clearPollTimer();

    if (pollInProgress) {
        refreshPending = true;
        return;
    }

    refreshDeviceStates();
}

async function refreshDeviceStates() {
    if (shuttingDown || pollInProgress) {
        return;
    }

    pollInProgress = true;
    refreshPending = false;
    clearPollTimer();

    let nextDelay = POLL_INTERVAL_MS;

    try {
        const readings = await batteryReader.readAllBatteries();
        enumerationFailureCount = 0;
        deviceManager.updateFromReadings(readings);
    } catch (error) {
        enumerationFailureCount += 1;
        nextDelay = RETRY_INTERVAL_MS;

        console.error(
            '[battery] Failed to enumerate Razer devices (' +
            enumerationFailureCount + '/' +
            MAX_ENUMERATION_FAILURES_BEFORE_DISCONNECT + '):',
            error,
        );

        if (enumerationFailureCount >= MAX_ENUMERATION_FAILURES_BEFORE_DISCONNECT) {
            deviceManager.markAllDisconnected();
        }
    } finally {
        pollInProgress = false;

        if (refreshPending) {
            refreshPending = false;
            schedulePoll(0);
        } else {
            schedulePoll(nextDelay);
        }
    }
}

function handleSystemResume() {
    if (resumeTimer) {
        clearTimeout(resumeTimer);
    }

    resumeTimer = setTimeout(() => {
        resumeTimer = null;
        requestImmediateRefresh();
    }, RESUME_REFRESH_DELAY_MS);
}

async function quitApplication() {
    if (shuttingDown) {
        return;
    }

    shuttingDown = true;
    clearPollTimer();

    if (resumeTimer) {
        clearTimeout(resumeTimer);
        resumeTimer = null;
    }

    powerMonitor.removeListener('resume', handleSystemResume);

    try {
        await batteryReader?.dispose();
    } catch (error) {
        console.warn('[usb] Failed to dispose battery reader:', error);
    }

    transport?.dispose();
    deviceManager?.removeAllListeners();
    trayController?.destroy();
    app.quit();
}

app.whenReady().then(() => {
    transport = new WebUsbTransport();
    batteryReader = new RazerBatteryReader({ transport });
    deviceManager = new DeviceManager();
    trayController = new TrayController({
        rootPath: app.getAppPath(),
        onRefresh: requestImmediateRefresh,
        onQuit: quitApplication,
    });

    deviceManager.on(DEVICE_EVENTS.DEVICES_CHANGED, renderDeviceStates);
    transport.on(TRANSPORT_EVENTS.DEVICES_CHANGED, requestImmediateRefresh);
    powerMonitor.on('resume', handleSystemResume);

    transport.startWatching();
    trayController.initialize();
    renderDeviceStates();
    requestImmediateRefresh();
});

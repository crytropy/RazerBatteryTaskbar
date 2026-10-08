const path = require('path');
const {
    Menu,
    Tray,
    nativeImage,
} = require('electron');

function formatDeviceLabel(device) {
    const name = device.name || 'Unknown Razer device';

    if (!device.connected) {
        return name + ' — Disconnected';
    }

    if (!Number.isFinite(device.battery)) {
        return name + ' — Battery unavailable';
    }

    return name + ' — ' + device.battery.toFixed(1) + '%';
}

class TrayController {
    constructor({ rootPath, onRefresh, onQuit }) {
        this.rootPath = rootPath;
        this.onRefresh = onRefresh;
        this.onQuit = onQuit;
        this.tray = null;
    }

    initialize() {
        const icon = nativeImage.createFromPath(this.getBatteryIconPath(0));
        this.tray = new Tray(icon);

        this.tray.setToolTip('Searching for Razer devices');
        this.tray.setTitle('Razer battery life');
        this.setDevices([], null);
    }

    setDevices(devices, primaryDevice) {
        if (!this.tray) {
            return;
        }

        this.updatePrimaryDisplay(primaryDevice);
        this.tray.setContextMenu(this.buildContextMenu(devices));
    }

    updatePrimaryDisplay(primaryDevice) {
        if (!primaryDevice?.connected) {
            this.tray.setImage(nativeImage.createFromPath(this.getBatteryIconPath(0)));
            this.tray.setToolTip('No supported Razer device detected');
            return;
        }

        if (!Number.isFinite(primaryDevice.battery)) {
            this.tray.setImage(nativeImage.createFromPath(this.getBatteryIconPath(0)));
            this.tray.setToolTip((primaryDevice.name || 'Razer device') + ': Battery unavailable');
            return;
        }

        const normalizedBattery = Math.min(100, Math.max(0, primaryDevice.battery));
        this.tray.setImage(nativeImage.createFromPath(this.getBatteryIconPath(normalizedBattery)));
        this.tray.setToolTip(
            (primaryDevice.name || 'Razer device') + ': ' + normalizedBattery.toFixed(1) + '%',
        );
    }

    buildContextMenu(devices) {
        const deviceItems = devices.length > 0
            ? devices.map(device => ({
                label: formatDeviceLabel(device),
                enabled: false,
            }))
            : [{
                label: 'No supported Razer devices',
                enabled: false,
            }];

        return Menu.buildFromTemplate([
            ...deviceItems,
            { type: 'separator' },
            { label: 'Refresh', type: 'normal', click: this.onRefresh },
            { type: 'separator' },
            { label: 'Quit', type: 'normal', click: this.onQuit },
        ]);
    }

    getBatteryIconPath(battery) {
        const iconLevel = Math.floor(battery / 10) * 10;
        return path.join(this.rootPath, 'src/assets/battery_' + iconLevel + '.png');
    }

    destroy() {
        if (this.tray) {
            this.tray.destroy();
            this.tray = null;
        }
    }
}

module.exports = {
    TrayController,
    formatDeviceLabel,
};

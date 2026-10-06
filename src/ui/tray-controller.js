const path = require('path');
const {
    Menu,
    Tray,
    nativeImage,
} = require('electron');

class TrayController {
    constructor({ rootPath, onQuit }) {
        this.rootPath = rootPath;
        this.onQuit = onQuit;
        this.tray = null;
    }

    initialize() {
        const icon = nativeImage.createFromPath(this.getBatteryIconPath(0));
        this.tray = new Tray(icon);

        const contextMenu = Menu.buildFromTemplate([
            { label: 'Quit', type: 'normal', click: this.onQuit },
        ]);

        this.tray.setContextMenu(contextMenu);
        this.tray.setToolTip('Searching for device');
        this.tray.setTitle('Razer battery life');
    }

    setBatteryState({ battery }) {
        if (!Number.isFinite(battery)) {
            this.setDisconnected();
            return;
        }

        const normalizedBattery = Math.min(100, Math.max(0, battery));
        this.tray.setImage(nativeImage.createFromPath(this.getBatteryIconPath(normalizedBattery)));
        this.tray.setToolTip(normalizedBattery.toFixed(1) + '%');
    }

    setDisconnected() {
        if (!this.tray) {
            return;
        }

        this.tray.setImage(nativeImage.createFromPath(this.getBatteryIconPath(0)));
        this.tray.setToolTip('Device disconnected');
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
};

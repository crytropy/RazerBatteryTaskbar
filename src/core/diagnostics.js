function formatHex(value, width = 4) {
    if (!Number.isInteger(value)) {
        return '----';
    }

    return value.toString(16).toUpperCase().padStart(width, '0');
}

function formatBattery(device) {
    return Number.isFinite(device.battery)
        ? device.battery.toFixed(1) + '%'
        : 'unavailable';
}

function formatCharging(device) {
    if (device.charging === true) {
        return 'yes';
    }

    if (device.charging === false) {
        return 'no';
    }

    return 'unknown';
}

function buildDiagnostics({
    appVersion,
    platform,
    arch,
    electronVersion,
    nodeVersion,
    transportStatus,
    devices = [],
    primaryDevice = null,
    lastScanAt = null,
    lastScanError = null,
    enumerationFailureCount = 0,
}) {
    const lines = [
        'RazerBatteryTaskbar diagnostics',
        'Version: ' + (appVersion || 'unknown'),
        'Platform: ' + (platform || 'unknown') + ' ' + (arch || 'unknown'),
        'Electron: ' + (electronVersion || 'unknown'),
        'Node: ' + (nodeVersion || 'unknown'),
        'Transport: ' + (transportStatus?.name || 'unknown'),
        'Hotplug watching: ' + (transportStatus?.hotplugWatching ? 'yes' : 'no'),
        'Last scan: ' + (lastScanAt || 'never'),
        'Enumeration failures: ' + enumerationFailureCount,
        'Last scan error: ' + (lastScanError || 'none'),
        'Primary device: ' + (primaryDevice?.name || 'none'),
        'Devices: ' + devices.length,
    ];

    for (const device of devices) {
        lines.push(
            '- ' +
            (device.name || 'Unknown Razer device') +
            ' [' + device.type + ']' +
            ' VID:' + formatHex(device.vendorId) +
            ' PID:' + formatHex(device.productId) +
            ' connected=' + (device.connected ? 'yes' : 'no') +
            ' battery=' + formatBattery(device) +
            ' charging=' + formatCharging(device),
        );
    }

    return lines.join('\n');
}

module.exports = {
    buildDiagnostics,
    formatHex,
};

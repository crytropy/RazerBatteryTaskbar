const BATTERY_REPORT_DATA_SIZE = 80;
const BATTERY_REPLY_SIZE = 90;
const BATTERY_LEVEL_OFFSET = 9;

function calculateCrc(buffer) {
    let crc = 0;

    for (let index = 2; index < buffer.length; index += 1) {
        crc ^= buffer[index];
    }

    return crc;
}

function buildBatteryRequest(transactionId) {
    if (!Number.isInteger(transactionId) || transactionId < 0x00 || transactionId > 0xff) {
        throw new TypeError('transactionId must be a byte value');
    }

    const header = Buffer.from([
        0x00,
        transactionId,
        0x00,
        0x00,
        0x00,
        0x02,
        0x07,
        0x80,
    ]);

    const crc = calculateCrc(header);

    return Buffer.concat([
        header,
        Buffer.alloc(BATTERY_REPORT_DATA_SIZE),
        Buffer.from([crc, 0x00]),
    ]);
}

function parseBatteryLevel(data) {
    if (!data || typeof data.getUint8 !== 'function' || data.byteLength <= BATTERY_LEVEL_OFFSET) {
        throw new Error('Invalid battery response');
    }

    return (data.getUint8(BATTERY_LEVEL_OFFSET) / 255) * 100;
}

module.exports = {
    BATTERY_REPLY_SIZE,
    buildBatteryRequest,
    calculateCrc,
    parseBatteryLevel,
};

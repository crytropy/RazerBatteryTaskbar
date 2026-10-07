const productDefinitions = require('./razer-products.json');

const RAZER_VENDOR_ID = 0x1532;
const VALID_DEVICE_TYPES = new Set(['mouse', 'headset', 'dock', 'dongle', 'unknown']);

function parseHexValue(value, fieldName) {
    if (typeof value !== 'string' || !/^0x[0-9a-f]+$/i.test(value)) {
        throw new TypeError(fieldName + ' must be a hexadecimal string');
    }

    return Number.parseInt(value, 16);
}

function buildProductDatabase(definitions) {
    const products = {};

    for (const definition of definitions) {
        const productId = parseHexValue(definition.productId, 'productId');
        const transactionId = parseHexValue(definition.transactionId, 'transactionId');

        if (!definition.name || typeof definition.name !== 'string') {
            throw new TypeError('Each Razer product requires a name');
        }

        if (!VALID_DEVICE_TYPES.has(definition.type)) {
            throw new TypeError('Unsupported device type for ' + definition.name);
        }

        if (transactionId < 0x00 || transactionId > 0xff) {
            throw new RangeError('transactionId must fit in one byte');
        }

        if (products[productId]) {
            throw new Error('Duplicate Razer product ID: ' + definition.productId);
        }

        products[productId] = Object.freeze({
            productId,
            name: definition.name,
            type: definition.type,
            transactionId,
        });
    }

    return Object.freeze(products);
}

const RAZER_PRODUCTS = buildProductDatabase(productDefinitions);

function getRazerProduct(productId) {
    return RAZER_PRODUCTS[productId];
}

function isSupportedRazerDevice(device) {
    return Boolean(
        device
        && device.vendorId === RAZER_VENDOR_ID
        && getRazerProduct(device.productId),
    );
}

module.exports = {
    RAZER_VENDOR_ID,
    RAZER_PRODUCTS,
    getRazerProduct,
    isSupportedRazerDevice,
};

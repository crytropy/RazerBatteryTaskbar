const RAZER_VENDOR_ID = 0x1532;

const RAZER_PRODUCTS = Object.freeze({
    0x00A4: { name: 'Razer Mouse Dock Pro', transactionId: 0x1f },
    0x00AA: { name: 'Razer Basilisk V3 Pro Wired', transactionId: 0x1f },
    0x00AB: { name: 'Razer Basilisk V3 Pro Wireless', transactionId: 0x1f },
    0x00B9: { name: 'Razer Basilisk V3 X HyperSpeed', transactionId: 0x1f },
    0x007C: { name: 'Razer DeathAdder V2 Pro Wired', transactionId: 0x3f },
    0x007D: { name: 'Razer DeathAdder V2 Pro Wireless', transactionId: 0x3f },
    0x009C: { name: 'Razer DeathAdder V2 X HyperSpeed', transactionId: 0x1f },
    0x00B3: { name: 'Razer Hyperpolling Wireless Dongle', transactionId: 0x1f },
    0x00B6: { name: 'Razer Deathadder V3 Pro Wired', transactionId: 0x1f },
    0x00B7: { name: 'Razer Deathadder V3 Pro Wireless', transactionId: 0x1f },
    0x0083: { name: 'Razer Basilsk X HyperSpeed', transactionId: 0x1f },
    0x0086: { name: 'Razer Basilisk Ultimate', transactionId: 0x1f },
    0x0088: { name: 'Razer Basilisk Ultimate Dongle', transactionId: 0x1f },
    0x008F: { name: 'Razer Naga v2 Pro Wired', transactionId: 0x1f },
    0x0090: { name: 'Razer Naga v2 Pro Wireless', transactionId: 0x1f },
    0x00A5: { name: 'Razer Viper V2 Pro Wired', transactionId: 0x1f },
    0x00A6: { name: 'Razer Viper V2 Pro Wireless', transactionId: 0x1f },
    0x007B: { name: 'Razer Viper Ultimate Wired', transactionId: 0x3f },
    0x0078: { name: 'Razer Viper Ultimate Wireless', transactionId: 0x3f },
    0x007A: { name: 'Razer Viper Ultimate Dongle', transactionId: 0x3f },
    0x0555: { name: 'Razer Blackshark V2 Pro RZ04-0453', transactionId: 0x3f },
    0x0528: { name: 'Razer Blackshark V2 Pro RZ04-0322', transactionId: 0x3f },
    0x00AF: { name: 'Razer Cobra Pro Wired', transactionId: 0x1f },
    0x00B0: { name: 'Razer Cobra Pro Wireless', transactionId: 0x1f },
});

function getRazerProduct(productId) {
    return RAZER_PRODUCTS[productId];
}

function isSupportedRazerDevice(device) {
    return device.vendorId === RAZER_VENDOR_ID && getRazerProduct(device.productId) !== undefined;
}

module.exports = {
    RAZER_VENDOR_ID,
    RAZER_PRODUCTS,
    getRazerProduct,
    isSupportedRazerDevice,
};

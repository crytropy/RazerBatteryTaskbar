const test = require('node:test');
const assert = require('node:assert/strict');

const {
    RAZER_PRODUCTS,
    getRazerProduct,
    isSupportedRazerDevice,
} = require('./razer-products');

test('loads the supported device database', () => {
    assert.equal(Object.keys(RAZER_PRODUCTS).length, 24);

    const product = getRazerProduct(0x00ab);
    assert.equal(product.name, 'Razer Basilisk V3 Pro Wireless');
    assert.equal(product.type, 'mouse');
    assert.equal(product.transactionId, 0x1f);
});

test('requires the real Razer vendor id', () => {
    assert.equal(
        isSupportedRazerDevice({ vendorId: 0x1532, productId: 0x00ab }),
        true,
    );

    assert.equal(
        isSupportedRazerDevice({ vendorId: 0x9999, productId: 0x00ab }),
        false,
    );
});

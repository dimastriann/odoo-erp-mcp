const test = require('node:test');
const assert = require('node:assert/strict');
const { clampPage, paginate } = require('./pagination.js');

const items = Array.from({ length: 57 }, (_, index) => index + 1);

test('default page size returns the first twenty rows', () => {
    const page = paginate(items, 1, 20);
    assert.equal(page.items.length, 20);
    assert.equal(page.items[0], 1);
    assert.equal(page.start, 1);
    assert.equal(page.end, 20);
    assert.equal(page.pageCount, 3);
});

test('supported page sizes and max page size keep ranges accurate', () => {
    for (const size of [20, 50, 100, 1000]) {
        const page = paginate(items, 1, size);
        assert.equal(page.pageSize, size);
        assert.equal(page.items.length, Math.min(size, items.length));
        assert.equal(page.end, Math.min(size, items.length));
    }
});

test('page numbers are clamped to the available range', () => {
    assert.equal(clampPage(0, 57, 20), 1);
    assert.equal(clampPage(99, 57, 20), 3);
    assert.equal(paginate(items, 99, 20).items[0], 41);
});

test('empty lists still expose a stable first page', () => {
    const page = paginate([], 4, 20);
    assert.deepEqual(page.items, []);
    assert.equal(page.page, 1);
    assert.equal(page.start, 0);
    assert.equal(page.end, 0);
});

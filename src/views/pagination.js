(function (root, factory) {
    if (typeof module === 'object' && module.exports) {
        module.exports = factory();
    } else {
        root.OdooAdminPagination = factory();
    }
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    function clampPage(page, total, pageSize) {
        const pageCount = Math.max(1, Math.ceil(total / pageSize));
        return Math.min(Math.max(1, Number(page) || 1), pageCount);
    }

    function paginate(items, page, pageSize) {
        const total = items.length;
        const size = Math.max(1, Number(pageSize) || 20);
        const safePage = clampPage(page, total, size);
        const startIndex = (safePage - 1) * size;
        const pageItems = items.slice(startIndex, startIndex + size);
        return {
            items: pageItems,
            page: safePage,
            pageSize: size,
            total,
            pageCount: Math.max(1, Math.ceil(total / size)),
            start: total === 0 ? 0 : startIndex + 1,
            end: Math.min(startIndex + size, total)
        };
    }

    return { clampPage, paginate };
});

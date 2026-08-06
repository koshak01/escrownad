// listing-form.js — progressive disclosure for /deals/new/
// Type chips first; only after pick show IP params + common fields.
// Canon: forge chips / Form.collectInputs → WS handler deals_save.

function assetTypeValue(form) {
    const hidden = form.querySelector('[data-chips-name="asset_type"] input[type="hidden"]');
    return (hidden?.value || '').trim();
}

function refreshListingForm(form) {
    const type = assetTypeValue(form);
    const rest = form.querySelector('[data-listing-rest]');
    const ip = form.querySelector('[data-listing-ip]');
    if (!rest) return;

    if (!type) {
        rest.hidden = true;
        return;
    }
    rest.hidden = false;
    if (ip) ip.hidden = type !== 'ip';
}

function bindListingForm(form) {
    if (!form || form.dataset.listingBound) return;
    form.dataset.listingBound = '1';

    refreshListingForm(form);

    // chips.js updates hidden on click but does not fire change — listen here
    form.addEventListener('click', (e) => {
        if (e.target.closest('[data-chips-name="asset_type"] .ds-chip:not([disabled])')) {
            requestAnimationFrame(() => refreshListingForm(form));
        }
    });

    // also if something sets hidden programmatically
    const typeHidden = form.querySelector('[data-chips-name="asset_type"] input[type="hidden"]');
    if (typeHidden) {
        const obs = new MutationObserver(() => refreshListingForm(form));
        obs.observe(typeHidden, { attributes: true, attributeFilter: ['value'] });
    }
}

// The platform's date-picker.js initialises flatpickr with a Russian locale.
// This product is English only, so the date field is re-initialised in English.
// The Y-m-d format is unchanged: the server parses YYYY-MM-DD.
function englishDatePickers(root) {
    if (typeof window.flatpickr === 'undefined') return;
    root.querySelectorAll('input[data-flatpickr]').forEach(el => {
        if (el.dataset.enLocale) return;
        el.dataset.enLocale = '1';
        if (el._flatpickr) el._flatpickr.destroy();
        window.flatpickr(el, {
            locale: 'default',
            dateFormat: 'Y-m-d',
            allowInput: true,
        });
    });
}

function initAll() {
    document.querySelectorAll('[data-listing-form]').forEach(form => {
        bindListingForm(form);
        englishDatePickers(form);
    });
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initAll);
} else {
    initAll();
}
document.addEventListener('html-replaced', initAll);

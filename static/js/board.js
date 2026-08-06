// board.js — the live board at /deals/.
//
// Typing in the search box sends a request; the server renders the table rows
// and returns ready HTML, which replaces #board-rows. There is no "search"
// button. The server builds the content; JS only sends the request and hands
// the reply to the platform dispatcher.

import ws from 'forge/ws';
import ui from 'forge/ui-actions';

const DEBOUNCE_MS = 180;

function boardState(bar) {
    const active = bar.querySelector('[data-side].is-on');
    return {
        scope: bar.dataset.scope || 'all',
        side: active ? active.dataset.side : '',
        q: (bar.querySelector('[data-board-search]')?.value || '').trim(),
    };
}

async function refresh(bar) {
    const state = boardState(bar);
    // race: only the most recent request is acted on
    const seq = (bar._seq = (bar._seq || 0) + 1);
    try {
        const resp = await ws.request('deals_search', state);
        if (seq !== bar._seq) return;
        ui.dispatch(resp);
        const rows = document.querySelector('#board-rows');
        const count = bar.querySelector('[data-board-count]');
        if (rows && count) {
            count.textContent = `${rows.dataset.shown} / ${rows.dataset.total}`;
        }
    } catch (e) {
        console.error('[board] search failed', e);
    }
}

function bind(bar) {
    if (bar.dataset.boardBound) return;
    bar.dataset.boardBound = '1';

    const input = bar.querySelector('[data-board-search]');
    if (input) {
        let timer = null;
        input.addEventListener('input', () => {
            clearTimeout(timer);
            timer = setTimeout(() => refresh(bar), DEBOUNCE_MS);
        });
        // Enter must not reload the page — the data is already current
        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') {
                e.preventDefault();
                clearTimeout(timer);
                refresh(bar);
            }
        });
    }

    bar.querySelectorAll('[data-side]').forEach((chip) => {
        chip.addEventListener('click', () => {
            bar.querySelectorAll('[data-side]').forEach((c) => c.classList.remove('is-on'));
            chip.classList.add('is-on');
            refresh(bar);
        });
    });
}

function initAll() {
    document.querySelectorAll('[data-board]').forEach(bind);
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initAll);
} else {
    initAll();
}
document.addEventListener('html-replaced', initAll);

// The page glue: the wizard pairs the browsers, and the moment a wire
// exists the todo app on this page attaches to it (the fields contract of
// ADR 0024). Everything about pairing lives in the wizard element.
import './pair-wizard.js';
import { attach } from '../@schuhkarton/uic-sync/sync.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';

const FIELDS = ['draft', 'editing', 'items', 'selected'];

document.querySelector('pair-wizard')?.addEventListener('wire', (event) => {
    const { wire, greet } = (event as CustomEvent<{ wire: Wire; greet: boolean }>).detail;
    void (async () => {
        await customElements.whenDefined('todo-app');
        const el = document.querySelector('todo-app');
        if (el) {
            attach(el, { fields: FIELDS, wire, greet });
        }
    })();
});

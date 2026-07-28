// The page glue: the wizard pairs the browsers, and the moment a wire
// exists the todo app on this page attaches to it (the fields contract of
// ADR 0013). Everything about pairing lives in the wizard element. A
// takeover replaces the wire (ADR 0032): the previous attachment detaches
// so only the live wire mirrors the app.
import './pair-wizard.js';
import { STATE_FIELDS } from '../@schuhkarton/lit-todo/todo-app.js';
import { attach } from '../@schuhkarton/uic-sync/sync.js';
import type { Attachment } from '../@schuhkarton/uic-sync/sync.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';

let attachment: Attachment | null = null;

document.querySelector('pair-wizard')?.addEventListener('wire', (event) => {
    const { wire, greet } = (event as CustomEvent<{ wire: Wire; greet: boolean }>).detail;
    void (async () => {
        await customElements.whenDefined('todo-app');
        const el = document.querySelector('todo-app');
        if (el) {
            attachment?.detach();
            attachment = attach(el, { fields: STATE_FIELDS, wire, greet });
        }
    })();
});

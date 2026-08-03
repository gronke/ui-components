// The page glue, pairing-first: the wizard owns the pairing screen, and
// the moment a wire exists the todo app attaches to it (the fields
// contract of ADR 0013) and the navbar + todo take the page over. A
// takeover replaces the wire (ADR 0032): the previous attachment detaches
// so only the live wire mirrors the app. The navbar's disconnect closes on
// purpose: back to a fresh pairing screen, no reload.
import './pair-wizard.js';
import '../@gronke/uic-sync/status-navbar.js';
import { STATE_FIELDS } from '../@gronke/lit-todo/todo-app.js';
import { attach } from '../@gronke/uic-sync/sync.js';
import type { Attachment } from '../@gronke/uic-sync/sync.js';
import type { Wire } from '../@gronke/uic-sync/wire.js';
import type { PanelMode } from '../@gronke/uic-sync/pair-panel.js';
import type { PairWizard } from './pair-wizard.js';
import type { StatusNavbar } from '../@gronke/uic-sync/status-navbar.js';

let attachment: Attachment | null = null;

const wizard = document.querySelector('pair-wizard') as PairWizard | null;
const navbar = document.querySelector('status-navbar') as StatusNavbar | null;
const bar = document.querySelector('.bar');
const todoPane = document.querySelector('.todo-pane');
const pairingPane = document.querySelector('.pairing-pane');

// The screen rule, the terminal's twin: the todo (with the navbar) shows
// while a wire stands or just dropped (red badge, disconnect as the way
// back) and the pairing screen owns every other mode. The wrappers are
// plain divs so the UA's [hidden] rule wins unopposed.
function applyScreen(mode: PanelMode): void {
    const todo = mode === 'connected' || mode === 'dropped';
    bar?.toggleAttribute('hidden', !todo);
    todoPane?.toggleAttribute('hidden', !todo);
    pairingPane?.toggleAttribute('hidden', todo);
    if (navbar && wizard) {
        navbar.connected = wizard.connected;
        navbar.status = wizard.status;
        navbar.address = location.host;
    }
    if (todo) {
        void customElements.whenDefined('todo-app').then(() => {
            const app = document.querySelector('todo-app') as { focusDraft?: () => void } | null;
            app?.focusDraft?.();
        });
    }
}

wizard?.addEventListener('mode-changed', (event) => {
    applyScreen((event as CustomEvent<PanelMode>).detail);
});

wizard?.addEventListener('wire', (event) => {
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

navbar?.addEventListener('disconnect', () => {
    attachment?.detach();
    attachment = null;
    wizard?.disconnect();
});

// A QR code that renders alike in both hosts (ADR 0029). In the browser it
// draws an SVG with an external library; on the terminal the same element
// mounts a native Rust widget by the `data-tui="qr"` marker, reading the
// value off the attribute. One element, two renderers: the QR the shared
// pairing panel renders (ADR 0029 now specifies it), the browser having an
// SVG path the terminal's Boa runtime does not.
import { html, LitElement } from 'lit';

export class QrCode extends LitElement {
    static properties = {
        data: {},
    };

    declare data: string;

    constructor() {
        super();
        this.data = '';
    }

    createRenderRoot(): this {
        return this;
    }

    render() {
        // One element serves both hosts: the terminal mounts its native QR
        // widget by the `data-tui` marker and reads the value off the
        // attribute; the browser ignores both and fills this container with
        // an SVG in `updated()`.
        return html`<div class="qr" data-tui="qr" value=${this.data}></div>`;
    }

    updated(): void {
        void this.paint();
    }

    /** Browser only. The QR library loads through a dynamic import so the
     * terminal's Boa module loader (which has no such specifier) never
     * resolves it: an un-run import stays off the static graph, and a run
     * under Boa rejects here and is swallowed, leaving the native widget. */
    private async paint(): Promise<void> {
        if (typeof document === 'undefined' || !this.data) {
            return;
        }
        const container = this.querySelector('.qr');
        if (!container) {
            return;
        }
        try {
            const qrcode = (await import('qrcode-generator')).default;
            const code = qrcode(0, 'L');
            code.addData(this.data);
            code.make();
            container.innerHTML = code.createSvgTag({ cellSize: 4, margin: 4 });
        } catch {
            // No SVG path here (running under Boa, or the library is missing);
            // the native widget or the link text carries the invite.
        }
    }
}
customElements.define('qr-code', QrCode);

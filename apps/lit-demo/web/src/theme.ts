// The terminal palette, in one place. `static styles` are the components'
// terminal-only layer (real lit never applies them without a shadow root;
// the mocked lit adopts them at define); each component composes this
// fragment first and colors through its custom properties, so the palette
// has a single home. The browser's palette stays Bootstrap's.
import { css, type CSSResult } from 'lit';

export const terminalTheme: CSSResult = css`
    :host {
        --tui-accent: #e5c07b;
        --tui-muted: #808a93;
        --tui-info: #6fb3d2;
        --tui-ok: #a3be8c;
    }
    .card-header {
        font-weight: bold;
        color: var(--tui-accent);
    }
`;

// The templating channel: what upstream's lit-html package produces, the
// html capture and the empty sentinel; `lit` re-exports these and the
// directives live under lit-html/directives/.

export { html, nothing } from './runtime.js';
export { html as svg } from './runtime.js';

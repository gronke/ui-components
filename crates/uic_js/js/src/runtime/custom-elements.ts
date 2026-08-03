// The custom-elements polyfill: upgrades over the retained tree, the
// terminal twin of customElements.define + the parser's upgrade pass.

import { instances, registry } from './state.js';

// The host creates the element node (with its markup attributes) before
// instantiating: attribute reads in constructors and connectedCallback see
// the real initial state.
export function mountAt(tag: string, handle: number): number {
    const cls = registry.get(tag);
    if (!cls) {
        throw new Error(`unknown custom element <${tag}>`);
    }
    const el = new cls();
    el.__node = handle;
    el.__upgradeOwnProperties();
    el.__syncFromAttributes();
    instances.set(handle, el);
    el.connectedCallback();
    return handle;
}

// A committed subtree may nest custom elements; whatever the registry
// knows upgrades once per node, and the upgrade recurses through each
// child's own commit. A parent re-commit swaps its subtree: replaced
// children upgrade fresh from their attributes (data flows down through
// attribute bindings; the serialize commit drops `.prop=`), while the
// stranded old instances stay in `instances` and render into detached
// nodes as no-ops.
export function upgradeDescendants(handle: number): void {
    for (const tag of registry.keys()) {
        for (const child of __uic_query(handle, tag)) {
            if (!instances.has(child)) {
                mountAt(tag, child);
            }
        }
    }
}

// The runtime's face: one import surface over the polyfill modules. The
// lit channel files and main.ts import here; each web-platform concept
// lives — and is tested — in its own module under runtime/ (the state
// leaf breaks the data cycles; element ⇄ events ⇄ focus cross-call only
// inside function bodies, the deferred shape the boa_quirks cycle canary
// pins).

export { instances, listenerFns, nothing, registry } from './runtime/state.js';
export {
    beginRender,
    css,
    endRender,
    html,
    releaseListeners,
    renderToString,
} from './runtime/serialize.js';
export type { TemplateResult } from './runtime/serialize.js';
export {
    accessorDescriptor,
    attributeName,
    collectProps,
    fromAttribute,
    hasPrototypeAccessor,
    installAccessor,
    installAccessors,
} from './runtime/properties.js';
export { closestFrom, queryAllNodes, wrapNode } from './runtime/element.js';
export { deliver } from './runtime/events.js';
export { focusNode } from './runtime/focus.js';
export { mountAt, upgradeDescendants } from './runtime/custom-elements.js';
export { installDialogs } from './runtime/dialogs.js';
export { installStorage } from './runtime/storage.js';

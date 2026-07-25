// The mocked lit's shared singletons — ESM modules instantiate once, so
// this leaf carries the state the old bootstrap closure held: the custom
// element registry, the live instances by node handle, and the template
// listener table every render populates. Keeping the data here breaks the
// import cycles the concept modules would otherwise need.

export const registry = new Map<string, any>();
export const instances = new Map<number, any>();

// Listener table: template `@event` bindings register the function here and
// the rendered HTML carries a `data-uic-l` marker per element.
export const listenerFns = new Map<number, { event: string; fn: Function; host: any }>();

export const nothing = Object.freeze({ __litNothing: true });

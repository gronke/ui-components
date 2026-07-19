// lit-html produces ifDefined. Upstream removes the attribute on
// undefined; the serialize commit renders it empty instead — presence
// selectors should use boolean bindings.

import { nothing } from '../../runtime.js';

export const ifDefined = (value: unknown): unknown => value ?? nothing;

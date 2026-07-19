// lit-html produces when: a plain conditional over the two cases.

import { nothing } from '../../runtime.js';

export const when = (
    condition: unknown,
    trueCase: () => unknown,
    falseCase?: () => unknown,
): unknown => (condition ? trueCase() : falseCase ? falseCase() : nothing);

// Browser behavior of <input-select>; mirrors the Rust InputSelectLogic impl
// in select.rs — keep both in sync.
import type { InputSelect } from './input-select.js';
import type { SelectOption } from './uic-runtime.js';

// A select commits null for the empty option once a `default` is present.
function allowNull(el: { default?: string | null }): boolean {
  return el.default !== undefined;
}

// Prepends the default-controlled null option, catalog rules: a string
// default labels it, any other set value leaves it blank, unset adds none.
export function withDefaultOption(
  options: SelectOption[],
  el: { default?: string | null },
): SelectOption[] {
  const list = [...options];
  if (typeof el.default === 'string') {
    list.unshift({ value: '', label: el.default });
  } else if (el.default !== undefined) {
    list.unshift({ value: '', label: '' });
  }
  return list;
}

export function selectOptions(el: InputSelect): SelectOption[] {
  return withDefaultOption(el.options, el);
}

// The select-facing value: null and undefined render as the empty option.
export function formValue(el: InputSelect): string {
  return el.value ?? '';
}

// Placeholder styling on the visible layer while the null option shows.
export function frontClass(el: InputSelect): string {
  const empty = el.value === '' || el.value == null;
  return allowNull(el) && empty ? 'default text-muted fst-italic' : '';
}

export function embeddedClass(el: InputSelect): string {
  return el.embedded ? 'bg-transparent border-0' : '';
}

export function onChange(el: InputSelect, e: Event): void {
  const raw = (e.target as HTMLSelectElement).value;
  if (raw === '' && allowNull(el)) {
    el.value = null;
  } else {
    el.value = raw;
  }
}

// The catalog normalizes in the value setter, so external writes get the
// same rule: with a `default` present, the empty string becomes null.
export function willUpdate(el: InputSelect, changed: Map<PropertyKey, unknown>): void {
  if (!changed.has('value')) {
    return;
  }
  if (el.value === '' && allowNull(el)) {
    el.value = null;
  }
}

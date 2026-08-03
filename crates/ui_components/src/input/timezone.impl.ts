// Browser behavior of <input-timezone>; mirrors the Rust InputTimezoneLogic
// impl in timezone.rs; keep both in sync.
//
// The two targets keep specialized zone lists on purpose, side by side for
// comparison: timezone.rs iterates chrono-tz, this file asks the browser via
// Intl.supportedValuesOf. Both pin UTC first and shorten to the last path
// segment.
import type { InputTimezone } from './input-timezone.js';
import type { SelectOption } from './uic-runtime.js';
import * as select from './input-select.impl.js';

// Keep in sync with TIMEZONE_OPTIONS in timezone.rs.
const timezoneOptions: SelectOption[] = [
  'UTC',
  ...Intl.supportedValuesOf('timeZone').filter((zone) => zone !== 'UTC'),
].map((zone) => ({ value: zone, short: zone.split('/').pop()!.trim() }));

export function selectOptions(el: InputTimezone): SelectOption[] {
  return select.withDefaultOption(timezoneOptions, el);
}

// The shared select-family computeds delegate to the select implementation.
export const formValue = select.formValue as (el: InputTimezone) => string;
export const frontClass = select.frontClass as (el: InputTimezone) => string;
export const embeddedClass = select.embeddedClass as (el: InputTimezone) => string;
export const willUpdate = select.willUpdate as (
  el: InputTimezone,
  changed: Map<PropertyKey, unknown>,
) => void;

// Unlike the generic select, the empty selection is always null (the
// catalog's InputTimezone.onChange override).
export function onChange(el: InputTimezone, e: Event): void {
  const raw = (e.target as HTMLSelectElement).value;
  el.value = raw === '' ? null : raw;
}

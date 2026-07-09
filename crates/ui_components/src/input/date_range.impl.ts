// Browser behavior of <input-date-range>; mirrors the Rust
// InputDateRangeLogic impl in date_range.rs — keep both in sync.
import type { InputDateRange } from './input-date-range.js';

// The children draw their own borders; the shared chrome's group renders
// borderless around them.
export function connected(el: InputDateRange): void {
  el.seamless = true;
}

// Routed from the start child's value-changed binding.
export function onStartChanged(el: InputDateRange, e: Event): void {
  const detail = (e as CustomEvent).detail as { value?: string | null };
  el.start = detail.value ?? '';
}

// Routed from the end child's value-changed binding.
export function onEndChanged(el: InputDateRange, e: Event): void {
  const detail = (e as CustomEvent).detail as { value?: string | null };
  el.end = detail.value ?? '';
}

// Both ends in → the ISO interval; anything less commits empty.
function interval(start: string, end: string): string {
  return start && end ? `${start}/${end}` : '';
}

// The synchronization: the edited end pulls the other along when the range
// would invert (ISO dates order lexicographically), then the combined value
// derives from the ends; an external value write decomposes instead.
// Property writes here join the same update, like the Rust will_update.
export function willUpdate(el: InputDateRange, changed: Map<PropertyKey, unknown>): void {
  if (changed.has('start') || changed.has('end')) {
    if (el.start && el.end && el.end < el.start) {
      if (changed.has('start')) {
        el.end = el.start;
      } else {
        el.start = el.end;
      }
    }
    const value = interval(el.start, el.end);
    if (value !== el.value) {
      el.value = value;
    }
  } else if (changed.has('value')) {
    const [start = '', rest = ''] = (el.value ?? '').split('/');
    const end = start && rest && rest < start ? start : rest;
    if (el.start !== start) {
      el.start = start;
    }
    if (el.end !== end) {
      el.end = end;
    }
    // Normalizes malformed or inverted external writes.
    const value = interval(start, end);
    if (value !== el.value) {
      el.value = value;
    }
  }
}

// Post-commit: reflect whether the committed range is complete. The write
// schedules a follow-up update, like any reactive set in Lit's updated();
// the guard keeps that follow-up quiet.
export function updated(el: InputDateRange, changed: Map<PropertyKey, unknown>): void {
  if (!(changed.has('start') || changed.has('end') || changed.has('value'))) {
    return;
  }
  const complete = !!(el.start && el.end);
  if (el.complete !== complete) {
    el.complete = complete;
  }
}

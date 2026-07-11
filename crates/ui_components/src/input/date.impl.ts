// Browser behavior of <input-date>; mirrors the Rust InputDateLogic impl in
// date.rs — keep both in sync.
import { Temporal } from 'temporal-polyfill';
import { detailValue } from './uic-impl-helpers.js';
import type { InputDate } from './input-date.js';

// With the timezone select shown, the placeholder hints at the zone a bare
// date is interpreted in (the catalog's defaultPlaceholder).
export function placeholderText(el: InputDate): string {
  const base = el.placeholder || 'YYYY-MM-DD';
  return el.showTimezone ? `${base} · ${currentTimezone(el)}` : base;
}

// The embedded timezone select's default binding: the catalog passes
// defaultTimezone ?? "" so its null option always exists.
export function timezoneDefault(el: InputDate): string {
  return el.defaultTimezone ?? '';
}

// Routes the embedded select's value-changed into the timezone property
// (the catalog binds it via LitSync).
export function onTimezoneChanged(el: InputDate, e: Event): void {
  el.timezone = detailValue(e) as string | null;
}

function parseDate(raw: string): Temporal.PlainDate | null {
  const m = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(raw);
  if (!m) return null;
  try {
    return Temporal.PlainDate.from(
      { year: +m[1], month: +m[2], day: +m[3] },
      { overflow: 'reject' },
    );
  } catch {
    return null;
  }
}

// timezone ?? defaultTimezone ?? UTC (the catalog's currentTimezone chain).
function currentTimezone(el: InputDate): string {
  return el.timezone || el.defaultTimezone || 'UTC';
}

// Start of day in the current timezone; "compatible" disambiguation steps
// over DST gaps like the Rust side.
function startOfDay(el: InputDate, date: Temporal.PlainDate): Temporal.ZonedDateTime {
  return date.toZonedDateTime(currentTimezone(el));
}

// Port of the catalog's onUpdateDateOrTimezone: `date` wins over `value`;
// parse failures surface on the error line. Timezone-only changes are inert.
export function willUpdate(el: InputDate, changed: Map<PropertyKey, unknown>): void {
  if (changed.has('date')) {
    const value = el.date ? el.date.toPlainDate().toString() : '';
    if (value !== el.value) {
      el.value = value;
    }
  } else if (changed.has('value')) {
    if (!el.value) {
      if (el.date != null) {
        el.date = null;
      }
      el.errorMessage = undefined;
      el.error = false;
    } else {
      const date = parseDate(el.value);
      if (date) {
        const next = startOfDay(el, date);
        if (!el.date || !el.date.equals(next)) {
          el.date = next;
        }
        el.errorMessage = undefined;
        el.error = false;
      } else {
        el.errorMessage = `Invalid date: ${el.value}`;
        el.error = true;
      }
    }
  }
}

export function onChange(el: InputDate, e: Event): void {
  const input = e.target as HTMLInputElement;
  const raw = input.value.trim();
  if (raw === '') {
    el.value = '';
    el.date = null;
    el.errorMessage = undefined;
    el.error = false;
    return;
  }
  const date = parseDate(raw);
  if (!date) {
    el.errorMessage = `Invalid date: ${raw}`;
    el.error = true;
    return;
  }
  if (el.min) {
    const min = parseDate(el.min);
    if (min && Temporal.PlainDate.compare(date, min) < 0) {
      el.errorMessage = `Date before minimum ${el.min}`;
      el.error = true;
      return;
    }
  }
  if (el.max) {
    const max = parseDate(el.max);
    if (max && Temporal.PlainDate.compare(date, max) > 0) {
      el.errorMessage = `Date after maximum ${el.max}`;
      el.error = true;
      return;
    }
  }
  el.errorMessage = undefined;
  el.error = false;
  // Normalized (zero-padded) form, whatever spelling was typed; the zoned
  // date pins start of day in the current timezone.
  el.value = date.toString();
  el.date = startOfDay(el, date);
  if (input.value !== el.value) {
    input.value = el.value;
  }
}

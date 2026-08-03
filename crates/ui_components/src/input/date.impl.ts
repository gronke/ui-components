// Browser behavior of <input-date>; mirrors the Rust InputDateLogic impl in
// date.rs; keep both in sync.
import { Temporal } from 'temporal-polyfill';
import { detailValue } from './uic-impl-helpers.js';
import type { InputDate } from './input-date.js';

// The catalog's defaultPlaceholder: the variant's format hint (the minutes
// token is literally `ii` there), plus the zone a bare date is interpreted
// in when the timezone select shows.
export function placeholderText(el: InputDate): string {
  let base = el.placeholder;
  if (!base) {
    base = 'YYYY-MM-DD';
    if (!el.hideTime) {
      base += ' HH:ii';
      if (!el.hideSeconds) base += ':ss';
    }
  }
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

// The catalog's parseDate: a 1900–2099 year, then per stage one OPTIONAL
// separator and up to two OPTIONAL digits; a group with no digits still
// consumes its separator, so later parts match independently ("2024--05" →
// day 5). No end anchor: the first unrecognized character drops itself and
// everything after it. Out-of-range parts clamp (Temporal constrain);
// missing parts complete to the start of the period.
const PARSE_DATE =
  /^(?<year>(19|20)\d{2})(-?(?<month>\d{1,2})?)(-?(?<day>\d{1,2})?)((?: |T)?(?<hours>\d{1,2})?)(:?(?<minutes>\d{1,2})?)(:?(?<seconds>\d{1,2})?)/;

function parsePartial(raw: string): Temporal.PlainDateTime | null {
  const groups = PARSE_DATE.exec(raw)?.groups;
  if (!groups) return null;
  return Temporal.PlainDateTime.from(
    {
      year: +groups.year,
      month: groups.month ? +groups.month : 1,
      day: groups.day ? +groups.day : 1,
      hour: groups.hours ? Math.min(+groups.hours, 23) : 0,
      minute: groups.minutes ? Math.min(+groups.minutes, 59) : 0,
      second: groups.seconds ? Math.min(+groups.seconds, 59) : 0,
    },
    { overflow: 'constrain' },
  );
}

// The value format of the variant (the catalog's format getter).
function formatValue(el: InputDate, dt: Temporal.PlainDateTime): string {
  const date = dt.toPlainDate().toString();
  if (el.hideTime) return date;
  const pad = (n: number) => String(n).padStart(2, '0');
  const withMinutes = `${date} ${pad(dt.hour)}:${pad(dt.minute)}`;
  return el.hideSeconds ? withMinutes : `${withMinutes}:${pad(dt.second)}`;
}

// Snaps the completed instant to the variant's precision, at the start or
// the end of the period (the catalog's _reduceDatePrecision).
function reducePrecision(el: InputDate, dt: Temporal.PlainDateTime): Temporal.PlainDateTime {
  if (el.hideTime) {
    return dt.withPlainTime(el.endOf ? '23:59:59' : '00:00:00');
  }
  if (el.hideSeconds) {
    return dt.with({ second: el.endOf ? 59 : 0, millisecond: 0, microsecond: 0, nanosecond: 0 });
  }
  return dt;
}

// timezone ?? defaultTimezone ?? UTC (the catalog's currentTimezone chain).
function currentTimezone(el: InputDate): string {
  return el.timezone || el.defaultTimezone || 'UTC';
}

// The local wall clock in the current zone as a UTC-normalized instant;
// "compatible" disambiguation steps over DST gaps like the Rust side. The
// stored date is ALWAYS UTC; the zone only interprets the input.
function zoneLocalAsUtc(el: InputDate, dt: Temporal.PlainDateTime): Temporal.ZonedDateTime {
  return dt
    .toZonedDateTime(currentTimezone(el), { disambiguation: 'compatible' })
    .withTimeZone('UTC');
}

// Port of the catalog's onUpdateDateOrTimezone: `date` wins over `value`;
// an external `value` write derives the date but keeps the string as
// written (partials stay partial); parse failures surface on the error
// line. Timezone-only changes are inert.
export function willUpdate(el: InputDate, changed: Map<PropertyKey, unknown>): void {
  if (changed.has('date')) {
    // The UTC instant renders as the current zone's wall clock.
    el.value = el.date
      ? formatValue(el, el.date.withTimeZone(currentTimezone(el)).toPlainDateTime())
      : '';
  } else if (changed.has('value')) {
    if (!el.value) {
      if (el.date != null) {
        el.date = null;
      }
      el.errorMessage = undefined;
      el.error = false;
    } else {
      const local = parsePartial(el.value);
      if (local) {
        const next = zoneLocalAsUtc(el, reducePrecision(el, local));
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

// Strict YYYY-MM-DD, for the min/max bounds only.
function parseBound(raw: string): Temporal.PlainDate | null {
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

// Typed commits auto-complete (`2024` → `2024-01-01 00:00:00`) and echo the
// normalized string.
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
  const parsed = parsePartial(raw);
  if (!parsed) {
    el.errorMessage = `Invalid date: ${raw}`;
    el.error = true;
    return;
  }
  const local = reducePrecision(el, parsed);
  const date = local.toPlainDate();
  if (el.min) {
    const min = parseBound(el.min);
    if (min && Temporal.PlainDate.compare(date, min) < 0) {
      el.errorMessage = `Date before minimum ${el.min}`;
      el.error = true;
      return;
    }
  }
  if (el.max) {
    const max = parseBound(el.max);
    if (max && Temporal.PlainDate.compare(date, max) > 0) {
      el.errorMessage = `Date after maximum ${el.max}`;
      el.error = true;
      return;
    }
  }
  el.errorMessage = undefined;
  el.error = false;
  // The completed, zero-padded form, whatever spelling was typed; the zoned
  // date pins the UTC instant of that wall clock.
  el.value = formatValue(el, local);
  const next = zoneLocalAsUtc(el, local);
  if (!el.date || !el.date.equals(next)) {
    el.date = next;
  }
  if (input.value !== el.value) {
    input.value = el.value;
  }
}

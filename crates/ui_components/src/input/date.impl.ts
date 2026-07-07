// Browser behavior of <input-date>; mirrors the Rust InputDateLogic impl in
// date.rs — keep both in sync.
import type { InputDate } from './input-date.js';

export function placeholderText(el: InputDate): string {
  return el.placeholder || 'YYYY-MM-DD';
}

function parseDate(raw: string): Date | null {
  const m = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(raw);
  if (!m) return null;
  const date = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  const valid =
    date.getUTCFullYear() === +m[1] &&
    date.getUTCMonth() === +m[2] - 1 &&
    date.getUTCDate() === +m[3];
  return valid ? date : null;
}

function normalize(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`;
}

export function onChange(el: InputDate, e: Event): void {
  const input = e.target as HTMLInputElement;
  const raw = input.value.trim();
  if (raw === '') {
    el.value = '';
    el.errorMessage = undefined;
    return;
  }
  const date = parseDate(raw);
  if (!date) {
    el.errorMessage = `Invalid date: ${raw}`;
    return;
  }
  if (el.min) {
    const min = parseDate(el.min);
    if (min && date < min) {
      el.errorMessage = `Date before minimum ${el.min}`;
      return;
    }
  }
  if (el.max) {
    const max = parseDate(el.max);
    if (max && date > max) {
      el.errorMessage = `Date after maximum ${el.max}`;
      return;
    }
  }
  el.errorMessage = undefined;
  // Normalized (zero-padded) form, whatever spelling was typed.
  el.value = normalize(date);
  if (input.value !== el.value) {
    input.value = el.value;
  }
}

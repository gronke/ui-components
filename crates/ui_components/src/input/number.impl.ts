// Browser behavior of <input-number>; mirrors the Rust InputNumberLogic impl
// in number.rs; keep both in sync.
import type { InputNumber } from './input-number.js';

// Port of the catalog's getFloat: comma or dot decimals, dots as thousand
// separators when grouped in threes; null for anything malformed.
function getFloat(raw: string): number | null {
  let dots = 0;
  let commas = 0;
  let lastDotDistance = 0;
  for (let i = 0; i < raw.length; i++) {
    const c = raw[i];
    lastDotDistance++;
    switch (c) {
      case '.':
        if (commas > 0) return null;
        if (dots > 0 && lastDotDistance !== 4) return null;
        lastDotDistance = 0;
        dots++;
        break;
      case ',':
        commas++;
        break;
      case '0':
      case '1':
      case '2':
      case '3':
      case '4':
      case '5':
      case '6':
      case '7':
      case '8':
      case '9':
        break;
      case '-':
        if (i === 0) break;
        return null;
      default:
        return null;
    }
  }
  if (commas > 1) return null;
  let normalized = raw;
  if ((commas > 0 && dots > 0) || (commas === 0 && dots > 1)) {
    // Dots are thousand separators, e.g. `1.000,50` or `1.000.000`.
    normalized = normalized.replaceAll('.', '');
  }
  const number = parseFloat(normalized.replace(',', '.'));
  if (Number.isNaN(number)) return null;
  // Normalize -0 to 0 for consistency.
  return number === 0 ? 0 : number;
}

// Port of the catalog's getFixed: round half away from zero, comma as the
// decimal separator, whole numbers plain when decimals are optional.
function getFixed(value: number, decimals: number, decimalsOptional: boolean): string {
  const rounded = +(Math.round(parseFloat(value + `e+${decimals}`)) + `e-${decimals}`);
  if (decimalsOptional && Number.isInteger(rounded)) {
    return rounded.toString();
  }
  return rounded.toFixed(decimals).replace('.', ',');
}

function decimalsOf(el: InputNumber): number {
  return el.decimals >= 0 ? Math.trunc(el.decimals) : 2;
}

export function displayValue(el: InputNumber): string {
  if (el.value == null) return '';
  return getFixed(el.value, decimalsOf(el), el.decimalsOptional);
}

// Soft-keyboard hint; the catalog emits the invalid `number` token, this
// port uses the standard `numeric`.
export function inputMode(el: InputNumber): string {
  return decimalsOf(el) > 0 ? 'decimal' : 'numeric';
}

export function onChange(el: InputNumber, e: Event): void {
  const input = e.target as HTMLInputElement;
  const raw = input.value.trim();
  if (raw === '') {
    el.value = el.allowNull ? null : 0;
    el.errorMessage = undefined;
    el.error = false;
    input.value = displayValue(el);
    dispatchChange(el);
    return;
  }
  const number = getFloat(raw);
  if (number == null) {
    el.errorMessage = `Invalid number: ${raw}`;
    el.error = true;
    return;
  }
  el.value = number;
  el.errorMessage = undefined;
  el.error = false;
  // Echo the normalized display format into the input.
  input.value = displayValue(el);
  dispatchChange(el);
}

// Catalog parity: a native `change` CustomEvent next to the notify event
// (browser-only; the terminal relies on `value-changed`).
function dispatchChange(el: InputNumber): void {
  el.dispatchEvent(new CustomEvent('change', { detail: { value: el.value } }));
}

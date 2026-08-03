// Browser behavior of <input-text>; mirrors the Rust InputTextLogic impl in
// text.rs; keep both in sync.
import { trimmedValue } from './uic-impl-helpers.js';
import type { InputText } from './input-text.js';

export function onChange(el: InputText, e: Event): void {
  const input = e.target as HTMLInputElement;
  el.value = trimmedValue(input.value, el.allowNull);
  // Normalized echo, so the visible input matches the committed value.
  const visible = el.value ?? '';
  if (input.value !== visible) {
    input.value = visible;
  }
}

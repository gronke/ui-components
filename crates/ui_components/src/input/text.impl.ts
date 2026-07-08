// Browser behavior of <input-text>; mirrors the Rust InputTextLogic impl in
// text.rs — keep both in sync.
import type { InputText } from './input-text.js';

export function onChange(el: InputText, e: Event): void {
  const input = e.target as HTMLInputElement;
  const trimmed = input.value.trim();
  if (trimmed === '') {
    el.value = el.allowNull ? null : '';
  } else {
    el.value = trimmed;
  }
  // Normalized echo, so the visible input matches the committed value.
  const visible = el.value ?? '';
  if (input.value !== visible) {
    input.value = visible;
  }
}

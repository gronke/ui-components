// Browser behavior of <input-textarea>; mirrors the Rust InputTextareaLogic
// impl in textarea.rs — keep both in sync.
import { trimmedValue } from './uic-impl-helpers.js';
import type { InputTextarea } from './input-textarea.js';

export function onChange(el: InputTextarea, e: Event): void {
  const textarea = e.target as HTMLTextAreaElement;
  el.value = trimmedValue(textarea.value, el.allowNull);
}

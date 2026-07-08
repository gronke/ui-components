// Browser behavior of <input-textarea>; mirrors the Rust InputTextareaLogic
// impl in textarea.rs — keep both in sync.
import type { InputTextarea } from './input-textarea.js';

export function onChange(el: InputTextarea, e: Event): void {
  const textarea = e.target as HTMLTextAreaElement;
  const trimmed = textarea.value.trim();
  if (trimmed === '') {
    el.value = el.allowNull ? null : '';
    return;
  }
  el.value = trimmed;
}

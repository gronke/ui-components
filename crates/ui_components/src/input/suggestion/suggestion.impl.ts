// Browser behavior of <input-suggestion>; mirrors the Rust
// InputSuggestionLogic impl in mod.rs; keep both in sync. The dropdown
// half below is this target's popup painting, the twin of tui.rs
// (ADR 0002/0015).
import { trimmedValue } from './uic-impl-helpers.js';
import type { InputSuggestion } from './input-suggestion.js';

export function onInput(el: InputSuggestion, e: Event): void {
  el.query = (e.target as HTMLInputElement).value;
}

export function onChange(el: InputSuggestion, e: Event): void {
  const input = e.target as HTMLInputElement;
  el.value = trimmedValue(input.value, el.allowNull);
  // Normalized echo, so the visible input matches the committed value.
  const visible = el.value ?? '';
  if (input.value !== visible) {
    input.value = visible;
  }
  closeMenu(el);
}

// ---- the popup: rendered and keyboard-driven per target (ADR 0002) ----

/** The keyboard highlight; absent means Enter commits the typed text. */
const active = new WeakMap<InputSuggestion, number>();
/** connectedCallback re-fires on every re-attachment; wire once. */
const wired = new WeakSet<InputSuggestion>();

export function connected(el: InputSuggestion): void {
  if (wired.has(el)) {
    return;
  }
  wired.add(el);
  el.addEventListener('keydown', e => onKeydown(el, e as KeyboardEvent));
  el.addEventListener('focusout', () => closeMenu(el));
}

/** Rebuilds the dropdown rows whenever the host delivers new suggestions. */
export function updated(el: InputSuggestion, changed: Map<PropertyKey, unknown>): void {
  if (!changed.has('suggestions')) {
    return;
  }
  const menu = menuOf(el);
  const input = controlOf(el);
  if (!menu || !input) {
    return;
  }
  active.delete(el);
  menu.textContent = '';
  for (const option of el.suggestions) {
    const item = document.createElement('li');
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'dropdown-item';
    button.textContent = option.label || option.value;
    button.dataset.value = option.value;
    // mousedown beats focusout: preventDefault keeps the input focused.
    button.addEventListener('mousedown', ev => {
      ev.preventDefault();
      pick(el, option.value);
    });
    item.appendChild(button);
    menu.appendChild(item);
  }
  // Rows arriving while the user types open the popup, like the terminal.
  const open =
    el.suggestions.length > 0 && document.activeElement === input && input.value !== '';
  toggleMenu(el, open);
}

function onKeydown(el: InputSuggestion, e: KeyboardEvent): void {
  if (!isOpen(el)) {
    if (e.key === 'ArrowDown' && el.suggestions.length > 0) {
      toggleMenu(el, true);
      e.preventDefault();
    }
    return;
  }
  switch (e.key) {
    case 'ArrowDown':
      move(el, 1);
      e.preventDefault();
      break;
    case 'ArrowUp':
      move(el, -1);
      e.preventDefault();
      break;
    case 'Enter': {
      const index = active.get(el);
      if (index !== undefined && el.suggestions[index]) {
        pick(el, el.suggestions[index].value);
        e.preventDefault();
      } else {
        // Enter commits the typed text through the native change.
        closeMenu(el);
      }
      break;
    }
    case 'Escape':
      // Nothing to revert: the popup only moves a highlight.
      closeMenu(el);
      break;
  }
}

function move(el: InputSuggestion, delta: number): void {
  const items = [...menuOf(el)?.querySelectorAll('button.dropdown-item') ?? []];
  if (items.length === 0) {
    return;
  }
  const current = active.get(el);
  let next: number | undefined;
  if (current === undefined) {
    next = delta > 0 ? 0 : undefined;
  } else if (current + delta < 0) {
    next = undefined;
  } else {
    next = Math.min(current + delta, items.length - 1);
  }
  items.forEach((item, index) => item.classList.toggle('active', index === next));
  if (next === undefined) {
    active.delete(el);
  } else {
    active.set(el, next);
    items[next].scrollIntoView({ block: 'nearest' });
  }
}

/** A pick fills the input and commits through the same change route. */
function pick(el: InputSuggestion, value: string): void {
  const input = controlOf(el);
  if (!input) {
    return;
  }
  input.value = value;
  input.dispatchEvent(new Event('change', { bubbles: true }));
  closeMenu(el);
}

function controlOf(el: InputSuggestion): HTMLInputElement | null {
  return el.querySelector('[data-qa="suggestion-input"]');
}

function menuOf(el: InputSuggestion): HTMLUListElement | null {
  return el.querySelector('[data-qa="suggestion-list"]');
}

function isOpen(el: InputSuggestion): boolean {
  return menuOf(el)?.classList.contains('show') ?? false;
}

function toggleMenu(el: InputSuggestion, open: boolean): void {
  menuOf(el)?.classList.toggle('show', open);
  controlOf(el)?.setAttribute('aria-expanded', String(open));
  if (!open) {
    active.delete(el);
  }
}

function closeMenu(el: InputSuggestion): void {
  toggleMenu(el, false);
}

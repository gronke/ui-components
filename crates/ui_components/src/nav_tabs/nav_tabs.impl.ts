// Browser behavior of <nav-tabs>; mirrors the Rust NavTabsLogic impl in
// mod.rs; keep both in sync. The button rows below are this target's tab
// painting, the twin of tui.rs (ADR 0002/0015).
import type { NavTabs } from './nav-tabs.js';
import type { SelectOption } from './uic-runtime.js';

export function onInput(el: NavTabs, e: Event): void {
  const value = (e.target as HTMLElement | null)?.dataset?.value;
  if (typeof value === 'string') {
    el.value = value;
  }
}

/** Rebuilds the button rows on new options; the highlight tracks value. */
export function updated(el: NavTabs, changed: Map<PropertyKey, unknown>): void {
  if (!changed.has('options') && !changed.has('value')) {
    return;
  }
  const list = listOf(el);
  if (!list) {
    return;
  }
  if (changed.has('options')) {
    rebuild(list, el.options);
  }
  markActive(list, el.value);
}

function rebuild(list: HTMLUListElement, options: SelectOption[]): void {
  list.textContent = '';
  for (const option of options) {
    const item = document.createElement('li');
    item.className = 'nav-item';
    item.setAttribute('role', 'presentation');
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'nav-link';
    button.setAttribute('role', 'tab');
    button.textContent = option.short || option.label || option.value;
    button.dataset.value = option.value;
    // The click leaves as a bubbling input event, so the pick rides the
    // same @input binding the terminal widget commits through.
    button.addEventListener('click', () => {
      button.dispatchEvent(new Event('input', { bubbles: true }));
    });
    item.appendChild(button);
    list.appendChild(item);
  }
}

/** Marks the row matching the value, falling back to the first tab. */
function markActive(list: HTMLUListElement, value: string): void {
  const buttons = [...list.querySelectorAll<HTMLButtonElement>('button.nav-link')];
  const index = Math.max(0, buttons.findIndex(button => button.dataset.value === value));
  buttons.forEach((button, at) => {
    button.classList.toggle('active', at === index);
    button.setAttribute('aria-selected', String(at === index));
  });
}

function listOf(el: NavTabs): HTMLUListElement | null {
  return el.querySelector('[data-qa="tab-bar"]');
}

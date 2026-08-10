// Browser behaviour of <uic-icon>: inject the named Material SVG (from the
// generated map) into the host so it themes through `currentColor` and sizes
// via CSS. The terminal twin (ui_components_tui, data-tui="icon") rasterizes
// the same SVG to Braille cells, so the Rust logic stays empty.
import { ICON_SVGS } from './uic-icons.js';
import type { UicIcon } from './uic-icon.js';

export function updated(el: UicIcon, changed: Map<PropertyKey, unknown>): void {
  if (!changed.has('name')) {
    return;
  }
  const target = el.querySelector('.uic-icon');
  if (target) {
    // A known name yields its SVG; an unknown one clears (no broken glyph).
    target.innerHTML = ICON_SVGS[el.name] ?? '';
  }
}

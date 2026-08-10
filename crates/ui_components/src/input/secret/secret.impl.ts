// Browser behavior of <input-secret>: the reveal toggle, the clipboard copy,
// and the value commit (onChange, mirroring on_change in mod.rs). Reveal and
// copy are wired by delegation on the host in `connected`; the eye/clipboard
// glyphs are <uic-icon>s whose `name` swaps. The buttons are browser-only
// chrome (the terminal twin draws its own reveal), so the template marks them
// `hidden` and `updated` un-hides them here.
import type { InputSecret } from './input-secret.js';

// connectedCallback re-fires on every re-attachment; wire once per element.
const wired = new WeakSet<InputSecret>();

function part(el: InputSecret, qa: string): HTMLElement | null {
  return el.querySelector(`[data-qa="${qa}"]`);
}

// Commit a new secret; mirrors on_change in mod.rs. Verbatim (no trim, a secret
// must not be altered); an empty entry clears to null (the "not set" state).
export function onChange(el: InputSecret, e: Event): void {
  const raw = (e.target as HTMLInputElement).value;
  el.value = raw === '' ? null : raw;
}

export function connected(el: InputSecret): void {
  if (wired.has(el)) {
    return;
  }
  wired.add(el);
  // Delegate on the host: a click bubbles up to `el`, which always exists even
  // before the first render (the input-suggestion pattern); the parts are
  // queried inside the handler, at click time.
  el.addEventListener('click', event => {
    const target = event.target as Element;
    if (target.closest('[data-qa="secret-reveal"]')) {
      toggleReveal(el);
    } else if (target.closest('[data-qa="secret-copy"]')) {
      copyValue(el);
    }
  });
}

// Reveal the browser-only reveal/copy buttons once the template has rendered
// (they ship `hidden` so the terminal never shows them).
export function updated(el: InputSecret): void {
  part(el, 'secret-reveal')?.removeAttribute('hidden');
  part(el, 'secret-copy')?.removeAttribute('hidden');
}

// Flip the masked input to plain text and back, swapping the eye glyph.
function toggleReveal(el: InputSecret): void {
  const input = part(el, 'secret-input') as HTMLInputElement | null;
  if (!input) {
    return;
  }
  const revealed = input.type !== 'text';
  input.type = revealed ? 'text' : 'password';
  part(el, 'secret-reveal')?.setAttribute('aria-pressed', String(revealed));
  part(el, 'secret-reveal-icon')?.setAttribute('name', revealed ? 'visibility_off' : 'visibility');
}

// Copy the value — it lives on the element property, not the masked input. A
// rejected write (insecure context, denied permission) is swallowed, leaving
// the value on screen to copy by hand.
function copyValue(el: InputSecret): void {
  void navigator.clipboard
    .writeText(el.value ?? '')
    .then(() => flashCopied(el))
    .catch(() => {});
}

// Briefly swap the clipboard glyph for a check, so the click has feedback.
const flashing = new WeakMap<InputSecret, ReturnType<typeof setTimeout>>();

function flashCopied(el: InputSecret): void {
  const icon = part(el, 'secret-copy-icon');
  if (!icon) {
    return;
  }
  icon.setAttribute('name', 'check');
  const previous = flashing.get(el);
  if (previous) {
    clearTimeout(previous);
  }
  flashing.set(
    el,
    setTimeout(() => {
      icon.setAttribute('name', 'content_copy');
      flashing.delete(el);
    }, 1200),
  );
}

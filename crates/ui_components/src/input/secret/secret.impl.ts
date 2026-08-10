// Browser behavior of <input-secret>: the reveal toggle, the clipboard copy,
// and the value commit (onChange, mirroring on_change in mod.rs). Reveal and
// copy are wired by delegation on the host in `connected` (the input-suggestion
// pattern); copy has no terminal equivalent, so it lives here. The terminal
// twin (ui_components_tui, data-tui="secret-input") masks, reveals, and edits.
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
  // Delegate on the host: connectedCallback can fire before the first render,
  // so the buttons may not exist yet — but a click bubbles up to `el`, which
  // always does (the input-suggestion pattern of listening on the element).
  // The parts are queried inside the handler, at click time, when they exist.
  el.addEventListener('click', event => {
    const target = event.target as Element;
    if (target.closest('[data-qa="secret-reveal"]')) {
      toggleReveal(el);
    } else if (target.closest('[data-qa="secret-copy"]')) {
      copyValue(el);
    }
  });
}

// Flip the masked input to plain text and back, swapping the eye icon.
function toggleReveal(el: InputSecret): void {
  const input = part(el, 'secret-input') as HTMLInputElement | null;
  if (!input) {
    return;
  }
  const revealed = input.type === 'text';
  input.type = revealed ? 'password' : 'text';
  part(el, 'secret-reveal')?.setAttribute('aria-pressed', String(!revealed));
  part(el, 'icon-eye')?.classList.toggle('d-none', !revealed);
  part(el, 'icon-eye-slash')?.classList.toggle('d-none', revealed);
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

// Briefly swap the clipboard icon for a check, so the click has feedback.
const flashing = new WeakMap<InputSecret, ReturnType<typeof setTimeout>>();

function flashCopied(el: InputSecret): void {
  const clip = part(el, 'icon-copy');
  const check = part(el, 'icon-check');
  if (!clip || !check) {
    return;
  }
  clip.classList.add('d-none');
  check.classList.remove('d-none');
  const previous = flashing.get(el);
  if (previous) {
    clearTimeout(previous);
  }
  flashing.set(
    el,
    setTimeout(() => {
      clip.classList.remove('d-none');
      check.classList.add('d-none');
      flashing.delete(el);
    }, 1200),
  );
}

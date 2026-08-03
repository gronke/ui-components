// Browser behavior of <app-root>; mirrors the Rust AppRootLogic impl in
// app_root.rs; keep both in sync.
import { detailValue } from './uic-impl-helpers.js';
import { InMemorySource } from './uic-connectors.js';
import type { AppRoot } from './app-root.js';
import type { SelectOption } from './uic-runtime.js';

// The demo's static data source: a pool of words behind the shared query
// interface (ADR 0014). Keep in sync with WORD_POOL in app_root.rs; the
// parity fixtures replay both sides.
const WORDS = [
  'apple',
  'apricot',
  'avocado',
  'banana',
  'blueberry',
  'cherry',
  'cranberry',
  'date',
  'elderberry',
  'fig',
  'grape',
  'grapefruit',
  'guava',
  'kiwi',
  'lemon',
  'lime',
  'mango',
  'melon',
  'orange',
  'papaya',
  'peach',
  'pear',
  'plum',
  'raspberry',
];
export const wordPool = InMemorySource.fromWords(WORDS);

// state[key], or the child's own default when the member is absent.
function member(el: AppRoot, key: string, missing: unknown): unknown {
  const value = el.state[key];
  return value === undefined ? missing : value;
}

export function date(el: AppRoot): unknown {
  return member(el, 'date', '');
}

export function start(el: AppRoot): unknown {
  return member(el, 'start', '');
}

export function end(el: AppRoot): unknown {
  return member(el, 'end', '');
}

export function note(el: AppRoot): unknown {
  return member(el, 'note', '');
}

// The number child's default is 0 (number.rs), not empty.
export function amount(el: AppRoot): unknown {
  return member(el, 'amount', 0);
}

export function pick(el: AppRoot): unknown {
  return member(el, 'pick', '');
}

export function essay(el: AppRoot): unknown {
  return member(el, 'essay', '');
}

export function zone(el: AppRoot): unknown {
  return member(el, 'zone', '');
}

export function word(el: AppRoot): unknown {
  return member(el, 'word', '');
}

// The missing member stays empty; the bar's fallback-to-first shows the
// Form tab, and the value-changed echo of a boot push would otherwise
// write `tab` into every boot state.
export function tab(el: AppRoot): unknown {
  return member(el, 'tab', '');
}

// Keep in sync with tab_options in app_root.rs.
export function tabOptions(_el: AppRoot): SelectOption[] {
  return [
    { value: 'form', short: 'Form' },
    { value: 'about', short: 'About' },
  ];
}

// Unknown tab values show the form, the bar's fallback-to-first rule.
export function showAbout(el: AppRoot): boolean {
  return member(el, 'tab', '') === 'about';
}

export function showForm(el: AppRoot): boolean {
  return !showAbout(el);
}

// Keep in sync with pick_options in app_root.rs.
export function pickOptions(_el: AppRoot): SelectOption[] {
  return [
    { value: 'Europe/Amsterdam', short: 'Amsterdam' },
    { value: 'Europe/Berlin', short: 'Berlin' },
    { value: 'America/New_York', short: 'New_York' },
    { value: 'Pacific/Auckland', short: 'Auckland' },
  ];
}

// One line of key: value pairs in key order, byte-identical to the Rust
// state_line (null prints empty, like the TUI's display_text).
export function stateLine(el: AppRoot): string {
  return Object.entries(el.state)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([key, value]) => `${key}: ${value ?? ''}`)
    .join(' · ');
}

// Clone-on-write member update; an equal value leaves state untouched.
// The guard matters: Lit's dirty check is reference-based, so an
// unconditional spread would re-fire state-changed on every child echo.
function setMember(el: AppRoot, key: string, e: Event): void {
  const value = detailValue(e);
  if (el.state[key] === value) {
    return;
  }
  el.state = { ...el.state, [key]: value };
}

export function onDate(el: AppRoot, e: Event): void {
  setMember(el, 'date', e);
}

export function onStart(el: AppRoot, e: Event): void {
  setMember(el, 'start', e);
}

export function onEnd(el: AppRoot, e: Event): void {
  setMember(el, 'end', e);
}

export function onNote(el: AppRoot, e: Event): void {
  setMember(el, 'note', e);
}

export function onAmount(el: AppRoot, e: Event): void {
  setMember(el, 'amount', e);
}

export function onPick(el: AppRoot, e: Event): void {
  setMember(el, 'pick', e);
}

export function onEssay(el: AppRoot, e: Event): void {
  setMember(el, 'essay', e);
}

export function onZone(el: AppRoot, e: Event): void {
  setMember(el, 'zone', e);
}

export function onWord(el: AppRoot, e: Event): void {
  setMember(el, 'word', e);
}

export function onTab(el: AppRoot, e: Event): void {
  setMember(el, 'tab', e);
}

// The slim wrapper (ADR 0014): the word input's live query, answered by the
// pool, genuinely async in this target, landing as a property write.
export async function onWordQuery(el: AppRoot, e: Event): Promise<void> {
  const text = detailValue(e);
  el.wordSuggestions = await wordPool.query(typeof text === 'string' ? text : '');
}

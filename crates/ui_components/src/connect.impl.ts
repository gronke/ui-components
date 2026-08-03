// The browser twin of `connect.rs` (ADR 0002, ADR 0014): the same query
// interface, async through Promises. Keep the InMemorySource matching rules
// in sync with the Rust side; the parity fixtures replay both.

import type { SelectOption } from './uic-runtime.js';

/** An async suggestion source: resolves matching rows for the typed text. */
export interface QuerySource {
  query(text: string): Promise<SelectOption[]>;
}

/**
 * The pool: a fixed option list matched case-insensitively by value prefix,
 * in pool order, capped at `limit` rows. The empty query yields no rows.
 */
export class InMemorySource implements QuerySource {
  private readonly options: SelectOption[];
  private readonly limit: number;

  constructor(options: SelectOption[], limit = 8) {
    this.options = options;
    this.limit = limit;
  }

  /** A pool of plain words, each becoming a label-free option. */
  static fromWords(words: readonly string[], limit = 8): InMemorySource {
    return new InMemorySource(words.map(value => ({ value })), limit);
  }

  query(text: string): Promise<SelectOption[]> {
    const needle = text.toLowerCase();
    if (needle === '') {
      return Promise.resolve([]);
    }
    return Promise.resolve(
      this.options
        .filter(option => option.value.toLowerCase().startsWith(needle))
        .slice(0, this.limit),
    );
  }
}

/** The simplest source: a provided method answers each query. */
export class MethodSource implements QuerySource {
  private readonly method: (text: string) => SelectOption[] | Promise<SelectOption[]>;

  constructor(method: (text: string) => SelectOption[] | Promise<SelectOption[]>) {
    this.method = method;
  }

  async query(text: string): Promise<SelectOption[]> {
    return this.method(text);
  }
}

/**
 * A remote source: fetches the URL with the encoded text spliced into its
 * colon-notation `:query` parameter (`/api/suggest?q=:query`). The response
 * body maps through `map`, defaulting to an array of strings or
 * option-shaped objects.
 */
export class FetchSource implements QuerySource {
  private readonly urlTemplate: string;
  private readonly init?: RequestInit;
  private readonly map: (body: unknown) => SelectOption[];

  constructor(
    urlTemplate: string,
    options: { init?: RequestInit; map?: (body: unknown) => SelectOption[] } = {},
  ) {
    this.urlTemplate = urlTemplate;
    this.init = options.init;
    this.map = options.map ?? defaultRows;
  }

  async query(text: string): Promise<SelectOption[]> {
    const url = this.urlTemplate.replace(/:query\b/g, encodeURIComponent(text));
    const response = await fetch(url, this.init);
    if (!response.ok) {
      throw new Error(`suggestion fetch failed: ${response.status}`);
    }
    return this.map(await response.json());
  }
}

function defaultRows(body: unknown): SelectOption[] {
  if (!Array.isArray(body)) {
    return [];
  }
  return body.map(entry =>
    typeof entry === 'string' ? { value: entry } : (entry as SelectOption),
  );
}

/**
 * The slim wrapper: answers an input's `query-changed` events from a source
 * by writing the resolved rows into its `suggestions` property. A late
 * response never clobbers a newer one. Returns the disposer.
 */
export function connectSuggestions(
  el: HTMLElement & { suggestions: SelectOption[] },
  source: QuerySource,
): () => void {
  let latest = 0;
  const listener = async (e: Event) => {
    const seq = ++latest;
    const detail = (e as CustomEvent).detail?.value;
    const rows = await source.query(typeof detail === 'string' ? detail : '');
    if (seq === latest) {
      el.suggestions = rows;
    }
  };
  el.addEventListener('query-changed', listener);
  return () => el.removeEventListener('query-changed', listener);
}

#!/usr/bin/env node
// Replays the Rust-generated parity fixtures against the compiled app-root
// twin: the same state must produce the same computed outputs in both
// targets. Stage the inputs first:
//   cargo test -p uic_codegen_web --test parity
import { readFileSync } from 'node:fs';

const parity = new URL('../crates/uic_codegen_web/tests/parity/', import.meta.url);
const impl = await import(new URL('build/app-root.impl.js', parity));
const { cases, suggest, breadcrumb } = JSON.parse(
  readFileSync(new URL('fixtures.json', parity), 'utf8'),
);

let failed = false;
for (const { state, expect } of cases) {
  const el = { state };
  const got = {
    stateLine: impl.stateLine(el),
    amount: impl.amount(el),
    date: impl.date(el),
    tab: impl.tab(el),
    showForm: impl.showForm(el),
    showAbout: impl.showAbout(el),
  };
  for (const [key, want] of Object.entries(expect)) {
    const have = got[key];
    if (have !== want) {
      console.error(
        `state ${JSON.stringify(state)} · ${key}: rust ${JSON.stringify(want)} != ts ${JSON.stringify(have)}`,
      );
      failed = true;
    }
  }
}

// The suggest fixtures replay the Rust pool's answers through the TS twin's
// InMemorySource — the connector half of the parity (ADR 0014).
for (const { query, expect } of suggest) {
  const rows = await impl.wordPool.query(query);
  const have = rows.map(row => row.value);
  if (JSON.stringify(have) !== JSON.stringify(expect)) {
    console.error(
      `suggest ${JSON.stringify(query)}: rust ${JSON.stringify(expect)} != ts ${JSON.stringify(have)}`,
    );
    failed = true;
  }
}

// The breadcrumb fixtures replay the trail decoration through the compiled
// nav-breadcrumb twin; the rows are objects, so the comparison canonicalizes
// their key order (the Rust side serializes maps sorted).
const trail = await import(new URL('build/nav-breadcrumb.impl.js', parity));
const canonical = value =>
  JSON.stringify(value, (key, member) =>
    member && typeof member === 'object' && !Array.isArray(member)
      ? Object.fromEntries(Object.entries(member).sort(([a], [b]) => (a < b ? -1 : 1)))
      : member,
  );
for (const { items, divider, expect } of breadcrumb) {
  const have = trail.crumbs({ items, divider });
  if (canonical(have) !== canonical(expect)) {
    console.error(
      `breadcrumb ${JSON.stringify(items)}: rust ${JSON.stringify(expect)} != ts ${JSON.stringify(have)}`,
    );
    failed = true;
  }
}

// The other connector variants, spot-checked: the method passthrough and a
// fetch against a stubbed globalThis.fetch (URL substitution + mapping).
const connectors = await import(new URL('build/uic-connectors.js', parity));
{
  const source = new connectors.MethodSource(text => [{ value: text.toUpperCase() }]);
  const rows = await source.query('hi');
  if (rows.length !== 1 || rows[0].value !== 'HI') {
    console.error(`MethodSource passthrough failed: ${JSON.stringify(rows)}`);
    failed = true;
  }
}
{
  let requested;
  globalThis.fetch = async url => {
    requested = url;
    return { ok: true, json: async () => ['alpha', { value: 'beta', label: 'Beta' }] };
  };
  const source = new connectors.FetchSource('/api/suggest?q=:query');
  const values = (await source.query('a b')).map(row => row.value);
  if (requested !== '/api/suggest?q=a%20b' || JSON.stringify(values) !== '["alpha","beta"]') {
    console.error(`FetchSource failed: url=${requested} rows=${JSON.stringify(values)}`);
    failed = true;
  }
}

if (failed) process.exit(1);
console.log(
  `parity: ${cases.length} cases, ${suggest.length} queries and ${breadcrumb.length} trails agree across targets`,
);

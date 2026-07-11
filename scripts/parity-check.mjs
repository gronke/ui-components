#!/usr/bin/env node
// Replays the Rust-generated parity fixtures against the compiled app-root
// twin: the same state must produce the same computed outputs in both
// targets. Stage the inputs first:
//   cargo test -p uic_codegen_web --test parity
import { readFileSync } from 'node:fs';

const parity = new URL('../crates/uic_codegen_web/tests/parity/', import.meta.url);
const impl = await import(new URL('build/app-root.impl.js', parity));
const { cases } = JSON.parse(readFileSync(new URL('fixtures.json', parity), 'utf8'));

let failed = false;
for (const { state, expect } of cases) {
  const el = { state };
  const got = {
    stateLine: impl.stateLine(el),
    amount: impl.amount(el),
    date: impl.date(el),
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
if (failed) process.exit(1);
console.log(`parity: ${cases.length} cases agree across targets`);

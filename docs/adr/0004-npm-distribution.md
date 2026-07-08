# ADR 0004: The web output is an npm-distributable artifact

## Decision

The generated web components are consumable without Rust: `cargo run -p uic_dist` builds `dist/npm/` — compiled lit ESM modules plus `.d.ts` declarations (oxc isolated declarations), `elements.css`, the Custom Elements Manifest, and a `package.json` (`type: module`, per-component `exports` with types, `lit` as peer dependency, `customElements` pointing at the manifest).
The whole tree is produced by the pure-Rust toolchain (`web_modules` typescript/dts/scss processors); no Node tooling runs.

## Why

Components defined here should serve plain-HTML/lit consumers who never touch Rust, and the repo should be able to `npm publish` eventually.
The generated modules import only the bare `lit` specifier, so any bundler, import map, or web_modules vendor tree can resolve them.

## Consequences

- The package name placeholder is `@schuhkarton/ui-components`; the version follows the workspace crate version.
- Importing a component module registers the custom element (side effect), so `sideEffects: true` is declared.
- The generated TypeScript must stay isolated-declarations-clean; the dist test fails otherwise.

## Publishing (release workflow)

`release.yml` runs on `v*` tags and on `workflow_dispatch`: it builds `dist/npm` via `cargo run -p uic_dist`, checks the tag against the package version, and rehearses with `npm publish --dry-run`.
The real publish step is present but commented out until the registry decision (public npmjs vs GitHub Packages) lands; flipping it on needs the `NPM_TOKEN` secret and, for npmjs provenance, a later `id-token: write` follow-up.
The npm CLI in that workflow is the single npm-tooling step in the repo — builds and vendoring stay pure Rust (`web_modules`/npm-utils, which are deliberately read-only against the registry).
The tree ships `publishConfig.access: public` (scoped packages default to restricted), `repository`/`homepage`/`bugs`, and a generated `README.md` for the registry page.
The release flow is: version-bump PR on `workspace.package.version` → merge → tag `vX.Y.Z` → workflow.

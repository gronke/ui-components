# ADR 0004: The web output is an npm-distributable artifact

## Decision

The generated web components are consumable without Rust.
`cargo run -p uic_dist` builds `dist/npm/`: compiled lit ESM modules plus `.d.ts` declarations (oxc isolated declarations), the impl twins where a definition carries one, `elements.css`, the Custom Elements Manifest, a generated `README.md`, and a `package.json` (`type: module`, per-component `exports` with types plus entries for the stylesheet and the manifest, `lit` as peer dependency, `customElements` pointing at the manifest).
The whole tree is produced by the pure-Rust toolchain (`web_modules` typescript/dts/scss processors); no Node tooling runs.
The publish view is the catalog: components with `dist = false` stay out (the demo composition `app-root`, ADR 0013).
The same command emits the worker host tree beside it (`dist/npm-worker`, `@gronke/uic-worker`, ADR 0007) and the sync tooling tree (`dist/npm-sync`, `@gronke/uic-sync`, ADR 0013).

Until the registry decision (public npmjs vs GitHub Packages) lands, distribution is a **GitHub Release asset**: `release-asset.yml` runs on a published Release, builds the trees via `cargo run -p uic_dist`, checks the release tag against the package version, rehearses `npm publish --dry-run`, then `npm pack`s the component tree and attaches the `.tgz` to the Release.
A consumer vendors that asset directly — web_modules resolves a `.tgz` URL as a tarball source — without a registry.
`release.yml` remains a manual `workflow_dispatch` rehearsal against the current main (build the trees, `npm publish --dry-run`); the real `npm publish` is present but commented out and needs the `NPM_TOKEN` secret when the registry decision lands.
The npm CLI in those workflows is the single npm-tooling step in the repo; builds and vendoring stay pure Rust (`web_modules`/npm-utils, which are deliberately read-only against the registry).
The tree ships `publishConfig.access: public` (scoped packages default to restricted) and `repository`/`homepage`/`bugs` for the registry page.
The release flow is: version-bump PR on `workspace.package.version` → merge → cut a GitHub Release `vX.Y.Z` in the UI → `release-asset.yml` attaches the tarball.

## Why

Components defined here should serve plain-HTML/lit consumers who never touch Rust, and the repo should be able to `npm publish` eventually.
The generated modules import only the bare `lit` specifier, so any bundler, import map, or web_modules vendor tree can resolve them.

## Consequences

- The package name is `@gronke/ui-components`; the version follows the workspace crate version.
- Importing a component module registers the custom element (side effect), so `sideEffects: true` is declared.
- `lit` is pinned as `^3`; `temporal-polyfill` joins the peer dependencies exactly when a shipped component carries a Zoned property (ADR 0005).
- The generated TypeScript must stay isolated-declarations-clean; the declaration emit errors otherwise and the dist test fails.
- The publish rehearsal covers the component tree; the worker and sync trees ride the same build and publish decision.

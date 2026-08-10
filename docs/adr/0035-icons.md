# ADR 0035: icons are one vendored SVG source, rendered per target

## Decision

Icons in the catalog are a single vendored SVG source (`uic_icons`, from Material Symbols, `@material-symbols/svg-400`, Apache-2.0), surfaced through a `<uic-icon name="…">` component that renders both targets from those SVGs.
In the browser the named SVG is injected inline and themed through `currentColor`; in the terminal the `data-tui="icon"` twin rasterizes a build-time alpha mask of the same SVG to Braille cells, painted in the theme foreground.
The SVGs are committed (not fetched at build), so both targets read one hermetic source; `uic_icons`'s `build.rs` assembles the browser sprite and the web SVG map always, and — behind the `raster` feature — pre-rasterizes the masks with `resvg`, a **build-dependency** that never enters the runtime or wasm binary.

## Why

A terminal has no fonts, SVG, or image protocols — only unicode cells and colour — so an icon-font can never render there; an SVG rasterized to Braille (the `qr` widget's half-block technique, one step finer) is the only way to show a real icon in both targets from one source.
Committing the SVGs keeps the build hermetic and diff-reviewable and needs no npm at build; a font would add a runtime dependency (revising ADR 0034's "no icon-font dependency") for no terminal benefit, and per-icon fetching would make the source a moving target.
`resvg` stays a build-dependency so the frugality the repo keeps for `qrcode` holds: the runtime carries only the baked masks and a small downsampler, and a web-only build compiles no rasterizer at all (the `raster` feature is off there).
Colour is applied at paint (the theme foreground, `currentColor`'s terminal analogue), not baked into the mask, so the geometry cache is keyed only by name and cell box.

## Consequences

- A legible rasterized icon needs a few cells and up (Braille packs 2×4 subpixels per cell); at one or two cells any icon is a blob, by nature of the medium. `<uic-icon>` is for icons with room; small inline affordances (input-secret's reveal/copy) stay text/glyph in the terminal, and their web-only icon buttons ship `hidden` so the dual-target `<uic-icon>` never renders in the terminal there.
- The published package gains the vendored SVGs and the generated `uic-icons.ts` map (wired via `WebCodegen::extra_module`); a consumer of a component that embeds `<uic-icon>` includes that map, as it already includes the shared stylesheet.
- The icon set grows by committing another SVG under `uic_icons/svg/`; the sprite, the web map, and the masks regenerate from it.

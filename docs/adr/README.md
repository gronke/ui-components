# Architecture decision records

Every substantial decision lives here as one record; all of them are accepted and describe the status quo.
The overview tying the crates and the runtime together is [../architecture.md](../architecture.md).

| ADR | Decision |
|-----|----------|
| [0001](0001-expression-language.md) | The template expression language is closed |
| [0002](0002-per-target-behavior.md) | Behavior hooks are implemented once per target behind shared names |
| [0003](0003-catalog-migration.md) | Migrating the upstream catalog |
| [0004](0004-npm-distribution.md) | The web output is an npm-distributable artifact |
| [0005](0005-object-valued-properties.md) | Object-valued properties are a closed set, starting with `Zoned` |
| [0006](0006-select-options-are-data.md) | Select options are data, not template structure |
| [0007](0007-the-tui-runs-in-the-browser.md) | The TUI runs in the browser |
| [0008](0008-a-retained-dom-for-the-tui.md) | A retained DOM for the TUI |
| [0009](0009-composites-synchronize-in-will-update.md) | Composites synchronize in will_update |
| [0010](0010-templates-compile-to-parts.md) | Templates compile to parts |
| [0011](0011-components-mount-on-the-dom.md) | Components mount on the DOM |
| [0012](0012-the-paint-migration.md) | The paint migration |
| [0013](0013-app-state-synchronization.md) | App state is an object property, synchronized over a broadcast channel |
| [0014](0014-data-connectors.md) | Async data sources are connectors behind one query interface |
| [0015](0015-component-directories.md) | A component's directory holds all of its targets |
| [0016](0016-tui-compatibility-lint.md) | A linked lint validates TUI compatibility |
| [0017](0017-nav-tabs-and-the-card.md) | nav-tabs is a value-driven bar; the card is the bordered block |
| [0018](0018-template-iteration.md) | Template iteration over data rows |
| [0019](0019-table-layout.md) | Tables lay out as shared column tracks |
| [0020](0020-nav-breadcrumb.md) | A static breadcrumb trail |
| [0021](0021-the-class-map-becomes-a-stylesheet.md) | The class map becomes a stylesheet |
| [0022](0022-the-demo-becomes-a-gallery.md) | The demo becomes a gallery |
| [0023](0023-the-browser-hosts-its-own-runtime.md) | The browser hosts its own runtime |

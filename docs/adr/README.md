# Architecture decision records

Every record describes the status quo: a decision that still shapes the code today.
Records that a later decision absorbed live on in git history; the gaps in the numbering are theirs.
The overview tying the crates and the runtime together is [../architecture.md](../architecture.md).

## Foundations

| ADR | Decision |
|-----|----------|
| [0001](0001-expression-language.md) | The template expression language is closed |
| [0002](0002-per-target-behavior.md) | Behavior hooks are implemented once per target behind shared names |
| [0008](0008-a-retained-dom-for-the-tui.md) | A retained DOM for the TUI |
| [0021](0021-the-class-map-becomes-a-stylesheet.md) | The class map becomes a stylesheet |

## The component catalog

| ADR | Decision |
|-----|----------|
| [0003](0003-catalog-migration.md) | The catalog ports carry the upstream conventions, with recorded deviations |
| [0005](0005-object-valued-properties.md) | Object-valued properties are a closed set |
| [0014](0014-data-connectors.md) | Async data sources are connectors behind one query interface |
| [0017](0017-nav-tabs-and-the-card.md) | Structural HTML maps to the terminal: the tab bar, the card, tables and the breadcrumb |
| [0034](0034-secret-input.md) | input-secret is a display-only masked field, masked in both targets |
| [0035](0035-icons.md) | icons are one vendored SVG source, rendered per target |

## Terminal hosts and widgets

| ADR | Decision |
|-----|----------|
| [0007](0007-the-tui-runs-in-the-browser.md) | The TUI runs in the browser |
| [0026](0026-the-scripted-host-drives-native-widgets.md) | The scripted host drives native widgets |
| [0033](0033-the-scripted-host-emulates-browser-platform-apis.md) | The scripted host emulates browser platform APIs |

## State, pairing and sessions

| ADR | Decision |
|-----|----------|
| [0013](0013-app-state-synchronization.md) | State synchronizes as one canonical snapshot over one wire |
| [0028](0028-the-terminal-is-a-pairing-peer.md) | Pairing is a serverless mutual exchange |
| [0029](0029-the-pairing-ui-is-one-component.md) | The pairing UI is one shared component set |
| [0032](0032-sessions-hand-over-through-their-own-wire.md) | Sessions hand over through their own wire |

## Distribution

| ADR | Decision |
|-----|----------|
| [0004](0004-npm-distribution.md) | The web output is an npm-distributable artifact |

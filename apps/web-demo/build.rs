//! Bakes the demo frontend into `$OUT_DIR/dist`, which `main.rs` embeds with
//! `include_dir!`: generates the web components from the Rust catalog,
//! vendors the npm dependencies from `web/package.json`, writes the gallery
//! and one example page per manifest entry, and compiles all three roots
//! (`web/`, the generated components, the generated pages) in a single
//! `web_modules::build`.

use std::fs;
use std::path::PathBuf;

use web_modules::build::{build, BuildOptions};
use web_modules::vendor::specs_from_package_json;

/// One gallery entry: a component with its seeds, mirroring the terminal
/// demo's (`apps/tui-demo`). The page config carries what the shared boot
/// (`web/example.ts`) replays on both panes; notify pairs derive from the
/// registry.
struct Example {
    name: &'static str,
    /// The page path under the site root; the first segment is the gallery
    /// section (`demo`, `components`, `examples`).
    route: &'static str,
    tag: &'static str,
    title: &'static str,
    blurb: &'static str,
    attrs: &'static [(&'static str, &'static str)],
    /// Plain-valued property seeds, a JSON object.
    props_json: &'static str,
    /// Option-rows properties (ADR 0006), a JSON object of row arrays.
    option_props_json: &'static str,
    cols: u16,
    rows: u16,
    hint: &'static str,
    /// The whole-state broadcast channel; only the form sets one.
    channel: Option<&'static str>,
    /// Words answering the component's `query-changed` (an editable pool
    /// textarea on the page); only the suggestion example sets one.
    pool: Option<&'static [&'static str]>,
}

const EXAMPLES: &[Example] = &[
    Example {
        name: "form",
        route: "demo",
        tag: "app-root",
        title: "The form",
        blurb: "Every input in one component around one state object — commits trickle up, state trickles down, and browser tabs share it.",
        attrs: &[],
        props_json: r#"{"state": {"date": "2026-07-07 00:00:00", "start": "2026-07-07", "end": "2026-07-11"}}"#,
        option_props_json: "{}",
        cols: 72,
        rows: 58,
        hint: "Click a field or Tab around — Enter commits, F4 or a click opens pickers, Esc leaves.",
        channel: Some("uic-app-state"),
        pool: None,
    },
    Example {
        name: "input-date",
        route: "components/input-date",
        tag: "input-date",
        title: "Date input",
        blurb: "Partial dates complete themselves; the calendar and zone list open on F4.",
        attrs: &[
            ("label", "Date of purchase"),
            ("hint", "Format: YYYY-MM-DD"),
            ("hide-time", ""),
            ("value", "2026-07-07"),
            ("min", "2020-01-01"),
            ("max", "2030-12-31"),
            ("default-timezone", "Europe/Berlin"),
            ("show-timezone", ""),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 14,
        hint: "Enter commits · F4/Down opens the calendar or zone list.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-date-range",
        route: "components/input-date-range",
        tag: "input-date-range",
        title: "Date range",
        blurb: "Two dates, one interval: the end never precedes the start.",
        attrs: &[
            ("label", "Stay"),
            ("hint", "The end never precedes the start"),
            ("start", "2026-07-07"),
            ("end", "2026-07-11"),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 14,
        hint: "Enter commits an end; the other follows if the range inverts.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-text",
        route: "components/input-text",
        tag: "input-text",
        title: "Text input",
        blurb: "A single line, trimmed on commit; empty becomes null.",
        attrs: &[
            ("label", "Note"),
            ("hint", "Trimmed on commit; empty becomes null"),
            ("allow-null", ""),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 8,
        hint: "Type and Enter commits.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-number",
        route: "components/input-number",
        tag: "input-number",
        title: "Number input",
        blurb: "Locale-tolerant decimals with a unit suffix.",
        attrs: &[
            ("label", "Amount"),
            ("hint", "Comma or dot decimals; dots group thousands"),
            ("unit", "EUR"),
            ("allow-null", ""),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 8,
        hint: "Type an amount and Enter commits.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-textarea",
        route: "components/input-textarea",
        tag: "input-textarea",
        title: "Textarea",
        blurb: "Grows with its content up to max-lines; commits on leave.",
        attrs: &[
            ("label", "Comment"),
            ("hint", "Grows with its content up to max-lines"),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 16,
        hint: "Enter adds a line, Tab commits.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-select",
        route: "components/input-select",
        tag: "input-select",
        title: "Select",
        blurb: "Options are data rows, not markup; empty commits null once a default is set.",
        attrs: &[
            ("label", "Time zone"),
            ("hint", "Empty commits null once a default is set"),
            ("default", "Pick a zone"),
            ("value", "Europe/Berlin"),
        ],
        props_json: "{}",
        option_props_json: r#"{"options": [
            {"value": "Europe/Amsterdam", "short": "Amsterdam"},
            {"value": "Europe/Berlin", "short": "Berlin"},
            {"value": "America/New_York", "short": "New_York"},
            {"value": "Pacific/Auckland", "short": "Auckland"}
        ]}"#,
        cols: 64,
        rows: 14,
        hint: "F4/Down/Space opens the list · Enter picks, Esc reverts.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-timezone",
        route: "components/input-timezone",
        tag: "input-timezone",
        title: "Time zone",
        blurb: "The platform zone list behind the select, UTC first.",
        attrs: &[
            ("label", "Time zone"),
            ("hint", "The platform zone list, UTC first"),
            ("default", "Pick a zone"),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 14,
        hint: "F4/Down/Space opens the list · Enter picks, Esc reverts.",
        channel: None,
        pool: None,
    },
    Example {
        name: "input-suggestion",
        route: "components/input-suggestion",
        tag: "input-suggestion",
        title: "Suggestion",
        blurb: "Typeahead over rows the page answers from an editable word pool — both panes query the same source.",
        attrs: &[
            ("label", "Word"),
            ("hint", "Typeahead: the pool below answers query-changed"),
            ("placeholder", "start typing"),
            ("allow-null", ""),
        ],
        props_json: "{}",
        option_props_json: "{}",
        cols: 64,
        rows: 14,
        hint: "Type to query the pool; edit the pool and type again.",
        channel: None,
        pool: Some(&[
            "apple", "apricot", "avocado", "banana", "blueberry", "cherry", "cranberry",
            "date", "elderberry", "fig", "grape", "grapefruit", "guava", "kiwi", "lemon",
            "lime", "mango", "melon", "orange", "papaya", "peach", "pear", "plum",
            "raspberry",
        ]),
    },
    Example {
        name: "nav-tabs",
        route: "components/nav-tabs",
        tag: "nav-tabs",
        title: "Tabs",
        blurb: "A value-backed tab bar; the panes stay the host's job.",
        attrs: &[("value", "form")],
        props_json: "{}",
        option_props_json: r#"{"options": [
            {"value": "form", "short": "Form"},
            {"value": "about", "short": "About"},
            {"value": "settings", "short": "Settings"}
        ]}"#,
        cols: 64,
        rows: 6,
        hint: "Left/Right or a click switches the tab.",
        channel: None,
        pool: None,
    },
    Example {
        name: "uic-tree",
        route: "components/uic-tree",
        tag: "uic-tree",
        title: "Tree",
        blurb: "An expandable tree over nested rows; the collapse markers are generated content, drawn and rotated by the stylesheet in both targets.",
        attrs: &[],
        props_json: r#"{"nodes": [
            {"id": "documents", "label": "Documents", "children": [
                {"id": "reports", "label": "Reports", "children": [
                    {"id": "q3", "label": "Q3 Report"},
                    {"id": "q4", "label": "Q4 Report"}
                ]},
                {"id": "notes", "label": "Notes"}
            ]},
            {"id": "pictures", "label": "Pictures", "children": [
                {"id": "logo", "label": "logo.svg"}
            ]},
            {"id": "readme", "label": "README.md"}
        ]}"#,
        option_props_json: "{}",
        cols: 64,
        rows: 12,
        hint: "A click toggles a branch or selects a leaf.",
        channel: None,
        pool: None,
    },
    Example {
        name: "nav-breadcrumb",
        route: "components/nav-breadcrumb",
        tag: "nav-breadcrumb",
        title: "Breadcrumb",
        blurb: "A static trail from data rows; dividers are data, not CSS.",
        attrs: &[],
        props_json: r#"{"items": [
            {"label": "Documents", "href": "/documents"},
            {"label": "Reports", "href": "/documents/reports"},
            {"label": "Q3"}
        ]}"#,
        option_props_json: "{}",
        cols: 64,
        rows: 4,
        hint: "A static trail — nothing to focus.",
        channel: None,
        pool: None,
    },
];

/// The notify wiring of a tag, from the registry: every notifying property
/// with a JSON-faithful scalar type contributes its event and JS property
/// name. Rich types stay out — a Zoned crossing JSON arrives as a plain
/// string, not a Temporal instance, and the scalar `value` twin already
/// carries the same information.
fn notify_pairs(tag: &str) -> Vec<serde_json::Value> {
    use uic_core::JsType;
    let def = uic_core::CustomElementRegistry::get(tag)
        .unwrap_or_else(|| panic!("{tag} is not a registered component"));
    def.properties
        .iter()
        .filter(|property| {
            matches!(
                property.js_type,
                JsType::String | JsType::Number | JsType::Boolean
            )
        })
        .filter_map(|property| {
            property.notify_event_name().map(|event| {
                serde_json::json!({
                    "event": event.into_owned(),
                    "prop": property.js_name,
                })
            })
        })
        .collect()
}

/// The page's JSON config, the contract of `web/example-config.ts`.
fn config_json(example: &Example) -> String {
    let attrs: serde_json::Map<String, serde_json::Value> = example
        .attrs
        .iter()
        .map(|(name, value)| ((*name).to_string(), serde_json::json!(value)))
        .collect();
    let props: serde_json::Value =
        serde_json::from_str(example.props_json).expect("manifest props are valid JSON");
    let option_props: serde_json::Value = serde_json::from_str(example.option_props_json)
        .expect("manifest option rows are valid JSON");
    serde_json::json!({
        "tag": example.tag,
        "attrs": attrs,
        "props": props,
        "optionProps": option_props,
        "notify": notify_pairs(example.tag),
        "cols": example.cols,
        "rows": example.rows,
        "channel": example.channel,
        "pool": example.pool,
    })
    .to_string()
}

/// The shared head: theme-before-paint, the stylesheets, the importmap hole
/// web_modules fills per page. `base` anchors a nested page at the site
/// root, so the one importmap and every `./…` asset resolve from anywhere.
fn head(title: &str, base: &str) -> String {
    format!(
        r#"<meta charset="utf-8">
<script>
document.documentElement.setAttribute('data-bs-theme',
    localStorage.getItem('uic-theme')
        ?? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'));
</script>
{base}<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="./web_modules/bootstrap/dist/css/bootstrap.min.css">
<link rel="stylesheet" href="./web_modules/@xterm/xterm/css/xterm.css">
<link rel="stylesheet" href="./elements.css">
<link rel="stylesheet" href="./styles.css">
{{{{ importmap | safe }}}}"#
    )
}

fn example_page(example: &Example) -> String {
    let config = config_json(example);
    let depth = example.route.matches('/').count() + 1;
    let base = format!("<base href=\"{}\">\n", "../".repeat(depth));
    let pool_block = if example.pool.is_some() {
        r#"<section class="mt-4">
<label class="form-label small text-body-secondary" for="word-pool">word pool — the page answers query-changed from these rows, one word per line</label>
<textarea id="word-pool" class="form-control font-monospace" rows="5" data-qa="word-pool"></textarea>
</section>
"#
    } else {
        ""
    };
    let head = head(&format!("ui-components · {}", example.title), &base);
    format!(
        r##"<!doctype html>
<html lang="en" data-bs-theme="light">
<head>
{head}
<script type="module" src="./components/{tag}.js"></script>
<script type="module" src="./example.js"></script>
</head>
<body class="pt-5 bg-body-tertiary">
<div class="container-fluid px-4 px-xl-5">
<header class="d-flex align-items-center gap-3 mb-4">
<a href="./" class="text-decoration-none">&larr; examples</a>
<h1 class="h4 mb-0">{title}</h1>
<div class="ms-auto d-flex align-items-center gap-3">
<label class="d-none d-md-flex align-items-center gap-2 small text-body-secondary mb-0">width
<input id="pane-width" type="range" class="form-range" min="320" step="4">
</label>
<button id="theme-toggle" class="btn btn-sm btn-outline-secondary" data-qa="theme-toggle">&#9790;</button>
</div>
</header>
<ul class="nav nav-tabs d-md-none mb-3" role="tablist">
<li class="nav-item"><a class="nav-link active" href="#" data-pane-tab="web-pane">Web</a></li>
<li class="nav-item" id="tui-tab-item"><a class="nav-link" href="#" data-pane-tab="tui-pane">Terminal</a></li>
</ul>
<div id="panes" class="example-panes d-md-flex align-items-md-start gap-5">
<section id="web-pane" class="example-pane pane-active" data-qa="web-pane"></section>
<aside id="tui-pane" class="example-pane d-none" data-qa="tui-pane">
<div id="terminal" class="tui-screen"></div>
<p class="text-body-secondary small mt-2 mb-0">{hint}</p>
</aside>
</div>
{pool_block}</div>
<footer id="debug-bar" class="fixed-bottom bg-body border-top" data-qa="debug-bar">
<button id="debug-toggle" class="btn btn-sm w-100 py-0 text-body-secondary" aria-expanded="true" aria-controls="debug-body" title="Toggle the debug bar">&#9662;</button>
<div id="debug-body" class="debug-body container-fluid px-4 pb-2">
<h2 class="h6">notify events</h2>
<pre id="events" class="border bg-body p-2 small mb-0" data-qa="events"></pre>
</div>
</footer>
<script type="application/json" id="example-config">{config}</script>
</body>
</html>
"##,
        head = head,
        tag = example.tag,
        title = example.title,
        hint = example.hint,
        pool_block = pool_block,
        config = config,
    )
}

fn gallery_page() -> String {
    let head = head("ui-components", "");
    let section = |title: &str, blurb: &str, prefix: &str| -> String {
        let cards: String = EXAMPLES
            .iter()
            .filter(|example| {
                example.route == prefix || example.route.starts_with(&format!("{prefix}/"))
            })
            .map(|example| {
                format!(
                    r#"<div class="col">
<a class="card h-100 text-decoration-none" href="./{route}/">
<div class="card-body">
<h3 class="h5 card-title"><code>&lt;{tag}&gt;</code> — {title}</h3>
<p class="card-text text-body-secondary mb-0">{blurb}</p>
</div>
</a>
</div>
"#,
                    route = example.route,
                    tag = example.tag,
                    title = example.title,
                    blurb = example.blurb,
                )
            })
            .collect();
        if cards.is_empty() {
            return String::new();
        }
        format!(
            r#"<h2 class="h5 mt-5 mb-1">{title}</h2>
<p class="text-body-secondary">{blurb}</p>
<div class="row row-cols-1 row-cols-md-2 g-4 mb-4">
{cards}</div>
"#
        )
    };
    let sections = [
        section(
            "demo",
            "The composed form: every input around one state object, shared across panes and browser tabs.",
            "demo",
        ),
        section(
            "components",
            "One page per catalog component — the web component beside the same element in a terminal.",
            "components",
        ),
        section(
            "examples",
            "Maintained end-to-end examples.",
            "examples",
        ),
    ]
    .concat();
    format!(
        r#"<!doctype html>
<html lang="en" data-bs-theme="light">
<head>
{head}
<script type="module">
import {{ wireThemeToggle }} from './theme-mode.js';
wireThemeToggle(document.getElementById('theme-toggle'));
</script>
</head>
<body class="pt-5 bg-body-tertiary">
<div class="container px-4">
<header class="d-flex align-items-center mb-4">
<h1 class="h4 mb-0">ui-components</h1>
<button id="theme-toggle" class="btn btn-sm btn-outline-secondary ms-auto" data-qa="theme-toggle">&#9790;</button>
</header>
<p class="text-body-secondary">One Rust definition per component, rendered twice on every page: the
real web component beside the same element in a terminal. Slide the width, flip the theme, resize
the window — both variants respond.</p>
{sections}</div>
</body>
</html>
"#,
        head = head,
        sections = sections,
    )
}

fn main() {
    // Keep the catalog's inventory registrations linked into this build script.
    ui_components::link();

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let web = manifest.join("web");

    let generated = uic_codegen_web::WebCodegen::new(out.join("gen_web"))
        .manifest(true)
        .extra_module("uic-connectors.ts", ui_components::connect::WEB_TS)
        .run()
        .expect("generate web components from the Rust catalog");

    // The gallery and one page per example, generated from the manifest.
    let pages = out.join("gen_pages");
    let _ = fs::remove_dir_all(&pages);
    for example in EXAMPLES {
        let page = example_page(example);
        assert_eq!(
            page.matches("{{").count(),
            1,
            "exactly the importmap hole survives templating for {}",
            example.name
        );
        let dir = pages.join(example.route);
        fs::create_dir_all(&dir).expect("create example page dir");
        fs::write(dir.join("index.html.tera"), page).expect("write example page");
    }
    fs::create_dir_all(&pages).expect("create pages root");
    fs::write(pages.join("index.html.tera"), gallery_page()).expect("write gallery page");

    // Browser deps come from web/package.json `dependencies` (import-map
    // entries auto-derived from each package.json).
    let specs = specs_from_package_json(&web.join("package.json"))
        .expect("read browser dependencies from web/package.json");

    build(&BuildOptions {
        specs: &specs,
        roots: &[web, generated.root, pages],
        out: &out.join("dist"),
        // Document-relative importmap addresses: the gallery sits at the
        // site root and every example page anchors itself there with
        // <base href="../../">, so the same baked dist serves the dev
        // server, the embedded binary and a GitHub project page under
        // /<repo>/.
        mount: "./web_modules",
        html: "",
        template: None,
        processors: Default::default(),
        output: Default::default(),
    })
    .expect("build web-demo frontend");
}

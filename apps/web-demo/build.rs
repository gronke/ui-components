//! Bakes the demo frontend into `$OUT_DIR/dist`, which `main.rs` embeds with
//! `include_dir!`: generates the web components from the Rust catalog,
//! vendors the npm dependencies from `web/package.json`, writes the gallery
//! and one example page per manifest entry, and compiles all three roots
//! (`web/`, the generated components, the generated pages) in a single
//! `web_modules::build`.

use std::fs;
use std::path::{Path, PathBuf};

use web_modules::build::{build, BuildOptions};
use web_modules::templates::{render_file, Context};
use web_modules::vendor::{specs_from_package_json, vendor, PackageSpec};

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")
}

/// The shared head, rendered once per page: theme-before-paint, the
/// stylesheets, and the literal importmap hole web_modules fills when it
/// renders the emitted page (a raw block in the source template).
fn rendered_head(title: &str, depth: usize) -> String {
    let base = if depth == 0 {
        String::new()
    } else {
        format!("<base href=\"{}\">\n", "../".repeat(depth))
    };
    let mut context = Context::new();
    context.insert("title", title);
    context.insert("base", &base);
    render_file(&templates_dir().join("head.html.tera"), &context).expect("render head template")
}

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
    /// Option-rows properties (ADR 0005), a JSON object of row arrays.
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
        blurb: "Every input in one component around one state object: commits trickle up, state trickles down, and browser tabs share it.",
        attrs: &[],
        props_json: r#"{"state": {"date": "2026-07-07 00:00:00", "start": "2026-07-07", "end": "2026-07-11"}}"#,
        option_props_json: "{}",
        cols: 72,
        rows: 58,
        hint: "Click a field or Tab around: Enter commits, F4 or a click opens pickers, Esc leaves.",
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
        blurb: "Typeahead over rows the page answers from an editable word pool; both panes query the same source.",
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
        hint: "A static trail, nothing to focus.",
        channel: None,
        pool: None,
    },
];

/// A maintained end-to-end example: a foreign npm lit element rendered in
/// both panes: the browser pane through the real lit family, the terminal
/// pane through the dedicated worker on the browser's own engine (ADR
/// 0023). The page hands the session the entry inside the rewritten worker
/// module tree; entry and module list derive from the vendored tree at
/// build time.
struct ForeignExample {
    name: &'static str,
    route: &'static str,
    title: &'static str,
    blurb: &'static str,
    package: &'static str,
    range: &'static str,
    tag: &'static str,
    attrs: &'static [(&'static str, &'static str)],
    props_json: &'static str,
    cols: u16,
    rows: u16,
    hint: &'static str,
}

const FOREIGN_EXAMPLES: &[ForeignExample] = &[ForeignExample {
    name: "json-viewer",
    route: "examples/json-viewer",
    title: "Foreign element: json-viewer",
    blurb: "A third-party npm lit element, byte-unmodified in both panes: the real lit in the browser, the browser's own engine in a worker for the terminal, its stylesheet parsed into the cascade.",
    package: "@alenaksu/json-viewer",
    range: "^2",
    tag: "json-viewer",
    attrs: &[],
    props_json: r#"{"data": {"project": "gronke/ui-components", "renderers": 2, "panes": {"browser": "real lit", "terminal": "worker + mocked lit"}, "styled": true}}"#,
    cols: 72,
    rows: 18,
    hint: "Arrows navigate, Right/Left expand and collapse, a click toggles; the component's own code.",
}];

/// The vendored package's ESM entry per its manifest: `exports` "." →
/// `module` → `main` (the loader's own rule, mirrored for the build).
fn foreign_entry(package_root: &Path) -> String {
    fn export_target(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(path) => Some(path.clone()),
            serde_json::Value::Object(conditions) => ["import", "module", "default"]
                .iter()
                .find_map(|key| conditions.get(*key).and_then(export_target)),
            _ => None,
        }
    }
    let manifest =
        fs::read_to_string(package_root.join("package.json")).expect("vendored package manifest");
    let json: serde_json::Value = serde_json::from_str(&manifest).expect("parse manifest");
    let entry = json
        .get("exports")
        .and_then(|exports| match exports {
            serde_json::Value::String(_) => export_target(exports),
            serde_json::Value::Object(map) => match map.get(".") {
                Some(dot) => export_target(dot),
                None => export_target(exports),
            },
            _ => None,
        })
        .or_else(|| {
            json.get("module")
                .and_then(|module| module.as_str())
                .map(str::to_string)
        })
        .or_else(|| {
            json.get("main")
                .and_then(|main| main.as_str())
                .map(str::to_string)
        })
        .expect("an ESM entry in the vendored manifest");
    entry.trim_start_matches("./").to_string()
}

/// Every `.js` module of the vendored tree, package-root-relative: what
/// the page fetches and registers with the Boa session.
fn foreign_modules(package_root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read vendored tree").flatten() {
        let path = entry.path();
        if path.is_dir() {
            foreign_modules(package_root, &path, out);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("js") {
            continue;
        }
        let relative = path
            .strip_prefix(package_root)
            .expect("under the package root")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        out.push(relative);
    }
}

/// The notify wiring of a tag, from the registry; the page registers its
/// `on_notify` callbacks from exactly this list. The per-property pane sync
/// carries JSON-faithful scalars only: rich types stay out, since a Zoned
/// crossing JSON arrives as a plain string, not a Temporal instance, and
/// the scalar `value` twin already carries the same information. A channel
/// example consumes whole-state snapshots instead, so its rich-typed notify
/// (the form's `state` object) must register regardless; filtering it away
/// would sever the TUI→page direction entirely.
fn notify_pairs(tag: &str, channel: bool) -> Vec<serde_json::Value> {
    use uic_core::JsType;
    let def = uic_core::CustomElementRegistry::get(tag)
        .unwrap_or_else(|| panic!("{tag} is not a registered component"));
    def.properties
        .iter()
        .filter(|property| {
            channel
                || matches!(
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
        "notify": notify_pairs(example.tag, example.channel.is_some()),
        "cols": example.cols,
        "rows": example.rows,
        "channel": example.channel,
        "pool": example.pool,
    })
    .to_string()
}

fn example_page(example: &Example) -> String {
    let depth = example.route.matches('/').count() + 1;
    let mut context = Context::new();
    context.insert(
        "head",
        &rendered_head(&format!("ui-components · {}", example.title), depth),
    );
    context.insert("component_script", &true);
    context.insert("foreign_import", "");
    context.insert("tag", example.tag);
    context.insert("title", example.title);
    context.insert("hint", example.hint);
    context.insert("pool", &example.pool.is_some());
    context.insert("config", &config_json(example));
    render_file(&templates_dir().join("example.html.tera"), &context).expect("render example page")
}

/// The foreign page's config: no notify wiring, and the worker pane's
/// module tree rides along.
fn foreign_config_json(example: &ForeignExample, entry: &str, modules: &[String]) -> String {
    let attrs: serde_json::Map<String, serde_json::Value> = example
        .attrs
        .iter()
        .map(|(name, value)| ((*name).to_string(), serde_json::json!(value)))
        .collect();
    let props: serde_json::Value =
        serde_json::from_str(example.props_json).expect("manifest props are valid JSON");
    serde_json::json!({
        "tag": example.tag,
        "attrs": attrs,
        "props": props,
        "optionProps": {},
        "notify": [],
        "cols": example.cols,
        "rows": example.rows,
        "foreign": {
            "package": example.package,
            "entry": entry,
            "modules": modules,
        },
    })
    .to_string()
}

fn foreign_page(example: &ForeignExample, entry: &str, modules: &[String]) -> String {
    let depth = example.route.matches('/').count() + 1;
    let mut context = Context::new();
    context.insert(
        "head",
        &rendered_head(&format!("ui-components · {}", example.title), depth),
    );
    context.insert("component_script", &false);
    context.insert("foreign_import", example.package);
    context.insert("tag", example.tag);
    context.insert("title", example.title);
    context.insert("hint", example.hint);
    context.insert("pool", &false);
    context.insert("config", &foreign_config_json(example, entry, modules));
    render_file(&templates_dir().join("example.html.tera"), &context).expect("render foreign page")
}

fn gallery_page() -> String {
    let card = |route: &str, tag: &str, title: &str, blurb: &str| serde_json::json!({"route": route, "tag": tag, "title": title, "blurb": blurb});
    let cards_for = |prefix: &str| -> Vec<serde_json::Value> {
        let catalog = EXAMPLES.iter().map(|e| (e.route, e.tag, e.title, e.blurb));
        let foreign = FOREIGN_EXAMPLES
            .iter()
            .map(|e| (e.route, e.tag, e.title, e.blurb));
        catalog
            .chain(foreign)
            .filter(|(route, ..)| *route == prefix || route.starts_with(&format!("{prefix}/")))
            .map(|(route, tag, title, blurb)| card(route, tag, title, blurb))
            .collect()
    };
    let sections = serde_json::json!([
        {
            "title": "demo",
            "blurb": "The composed form: every input around one state object, shared across panes and browser tabs.",
            "cards": cards_for("demo"),
        },
        {
            "title": "components",
            "blurb": "One page per catalog component: the web component beside the same element in a terminal.",
            "cards": cards_for("components"),
        },
        {
            "title": "examples",
            "blurb": "Maintained end-to-end examples: foreign npm elements in both panes, the terminal side on the browser's own engine in a worker.",
            "cards": cards_for("examples"),
        },
        {
            "title": "apps",
            "blurb": "A hand-written Lit app around the same runtime, deployed beside this gallery (locally: cargo run -p uic_lit_demo -- serve).",
            "cards": [card(
                "lit-demo/p2p",
                "todo-app",
                "pair two browsers over WebRTC",
                "The lit-todo app with symmetric link pairing: each side sends one link (QR, message or paste) and both connect, with no server carrying state or signaling.",
            )],
        },
    ]);
    let mut context = Context::new();
    context.insert("head", &rendered_head("ui-components", 0));
    context.insert("sections", &sections);
    render_file(&templates_dir().join("gallery.html.tera"), &context).expect("render gallery page")
}

fn main() {
    println!("cargo:rerun-if-changed=templates");
    // Keep the catalog's inventory registrations linked into this build script.
    ui_components::link();
    // The demo composition <app-root> ships out of the published tree but
    // rides the generated web catalog (dist = false, ADR 0013).
    ui_components_demo::link();

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let web = manifest.join("web");

    let generated = uic_codegen_web::WebCodegen::new(out.join("gen_web"))
        .manifest(true)
        .extra_module("uic-connectors.ts", ui_components::connect::WEB_TS)
        .extra_module("uic-icons.ts", uic_icons::WEB_TS)
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
    // Foreign examples: the packages pre-vendor here, the entry derives
    // from the real tree, and the worker module tree carries the mocked
    // runtime beside the package with its bare lit* imports rewritten:
    // import maps do not reach workers.
    let worker_root = out.join("gen_worker");
    let _ = fs::remove_dir_all(&worker_root);
    let worker_modules = worker_root.join("tui-worker/modules");
    if !FOREIGN_EXAMPLES.is_empty() {
        uic_worker::worker_runtime_tree(&uic_js::js_src_root(), &worker_modules);
    }
    let foreign_vendor = out.join("vendor_foreign");
    for example in FOREIGN_EXAMPLES {
        let package_root = foreign_vendor.join(example.package);
        if !package_root.join("package.json").is_file() {
            let spec = PackageSpec::npm(example.package, example.range);
            vendor(&foreign_vendor, "/vendor", &[spec]).expect("vendor the foreign package");
        }
        let entry = foreign_entry(&package_root);
        let mut modules = Vec::new();
        foreign_modules(&package_root, &package_root, &mut modules);
        modules.sort();
        let package_depth = example.package.split('/').count();
        uic_worker::rewrite_foreign_package(
            &package_root,
            &worker_modules.join(example.package),
            package_depth,
        );
        let page = foreign_page(example, &entry, &modules);
        assert_eq!(
            page.matches("{{").count(),
            1,
            "exactly the importmap hole survives templating for {}",
            example.name
        );
        let dir = pages.join(example.route);
        fs::create_dir_all(&dir).expect("create foreign page dir");
        fs::write(dir.join("index.html.tera"), page).expect("write foreign page");
    }
    fs::create_dir_all(&pages).expect("create pages root");
    fs::write(pages.join("index.html.tera"), gallery_page()).expect("write gallery page");

    // Browser deps come from web/package.json `dependencies` (import-map
    // entries auto-derived from each package.json).
    let specs = specs_from_package_json(&web.join("package.json"))
        .expect("read browser dependencies from web/package.json");

    build(&BuildOptions {
        specs: &specs,
        roots: &[
            web,
            generated.root,
            pages,
            worker_root,
            uic_worker::web_root(),
        ],
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

//! Any third-party lit element, interactive in a real terminal: the package
//! vendors at runtime, its entry derives from its own manifest, the mocked
//! lit renders it into the retained document, and its `static styles` parse
//! into the terminal cascade.
//!
//! ```sh
//! cargo run -p uic_js --example third_party
//!     # the build-time vendored @alenaksu/json-viewer, offline
//! cargo run -p uic_js --example third_party -- '@alenaksu/json-viewer@^2' json-viewer \
//!     --prop 'data={"hello":"world"}'
//!     # any npm spec + tag (network); attributes and JSON properties seed it
//! cargo run -p uic_js --example third_party -- my-pkg@^1 my-tag \
//!     --attr label=Demo --vendor-dir /tmp/uic-vendor
//! ```
//!
//! Arrows/Home/End forward to the component, a click hits the element under
//! the pointer, Esc quits. A component importing a module the mock does not
//! provide reports the gap and everything the runtime offers.

use std::path::{Path, PathBuf};

use uic_js::JsHost;
use uic_tui::crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use uic_tui::{crossterm, ratatui, KeyStroke};
use web_modules::vendor::{vendor, PackageSpec};

const SAMPLE: &str = include_str!("sample.json");

struct Args {
    spec: Option<String>,
    tag: Option<String>,
    attrs: Vec<(String, String)>,
    props: Vec<(String, String)>,
    vendor_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        spec: None,
        tag: None,
        attrs: Vec::new(),
        props: Vec::new(),
        vendor_dir: None,
    };
    let mut rest = std::env::args().skip(1);
    while let Some(arg) = rest.next() {
        let split = |value: String| -> Result<(String, String), String> {
            value
                .split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| format!("expected key=value, got {value:?}"))
        };
        match arg.as_str() {
            "--attr" => args
                .attrs
                .push(split(rest.next().ok_or("--attr needs key=value")?)?),
            "--prop" => args
                .props
                .push(split(rest.next().ok_or("--prop needs key=json")?)?),
            "--vendor-dir" => {
                args.vendor_dir = Some(PathBuf::from(
                    rest.next().ok_or("--vendor-dir needs a path")?,
                ));
            }
            _ if args.spec.is_none() => args.spec = Some(arg),
            _ if args.tag.is_none() => args.tag = Some(arg),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    Ok(args)
}

/// The navigation keys a generic element most likely understands.
const KEYS: [&str; 6] = [
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Home",
    "End",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args().map_err(|err| format!("{err} (see the module docs for usage)"))?;

    let mut host = JsHost::new()?;
    let tag = match (&args.spec, &args.tag) {
        (Some(spec), Some(tag)) => {
            // Runtime vendoring: the same registry-read-only machinery the
            // build script uses, pointed at any name@range.
            let vendor_dir = args
                .vendor_dir
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("uic-js-third-party"));
            let package_spec = PackageSpec::parse(spec);
            let package = spec.rsplit_once('@').map_or(spec.as_str(), |(name, _)| {
                if name.is_empty() {
                    spec.as_str()
                } else {
                    name
                }
            });
            eprintln!("vendoring {spec} into {}…", vendor_dir.display());
            vendor(&vendor_dir, "/vendor", &[package_spec])?;
            host.load_package(&vendor_dir, package)?;
            tag.clone()
        }
        (None, None) => {
            // Offline self-demo: the build-time vendored test component.
            host.load_package(
                Path::new(env!("UIC_JS_VENDOR_ROOT")),
                "@alenaksu/json-viewer",
            )?;
            "json-viewer".to_string()
        }
        _ => return Err("usage: third_party [name@range tag] [--attr k=v] [--prop k=json]".into()),
    };

    let mut attrs: Vec<(&str, &str)> = args
        .attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if args.spec.is_none() && args.props.is_empty() && attrs.is_empty() {
        attrs.push(("data", SAMPLE));
    }
    let node = host.mount(&tag, &attrs)?;
    for (name, json) in &args.props {
        host.set_prop(node, name, json)?;
    }
    host.focus(node)?;

    let mut terminal = ratatui::try_init()?;
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let result = run(&mut host, &mut terminal, &tag);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::try_restore()?;
    result
}

fn run(
    host: &mut JsHost,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    tag: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let status =
        format!("<{tag}> via Boa · arrows forward to the component, click hits, Esc quits");
    loop {
        let state = host.state.clone();
        terminal.draw(|frame| {
            let mut s = state.borrow_mut();
            s.dirty = false;
            let focused = s.focused;
            let mut area = frame.area();
            if area.height > 1 {
                let status_area = ratatui::layout::Rect {
                    y: area.y + area.height - 1,
                    height: 1,
                    ..area
                };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(status.as_str())
                        .style(ratatui::style::Style::new().dim()),
                    status_area,
                );
                area.height -= 1;
            }
            uic_tui::dom::paint_document(frame, area, &mut s.doc, focused);
        })?;

        match crossterm::event::read()? {
            Event::Key(key) => {
                if let Some(stroke) = KeyStroke::from_crossterm(&key) {
                    if stroke.is_quit() {
                        return Ok(());
                    }
                    if KEYS.contains(&stroke.key.as_str()) {
                        host.dispatch(&stroke)?;
                    }
                }
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => {
                let target = {
                    let state = host.state.borrow();
                    let mut area = terminal.get_frame().area();
                    area.height = area.height.saturating_sub(1);
                    uic_tui::dom::hit_test(&state.doc, area, column, row)
                };
                if let Some(target) = target {
                    host.click(target)?;
                }
            }
            _ => {}
        }
    }
}

//! Bakes the demo frontend into `$OUT_DIR/dist`, which `main.rs` embeds with
//! `include_dir!`: vendors the npm dependencies from `web/package.json`,
//! writes the generated-components root, and compiles both roots (`web/` and
//! the generated one) in a single `web_modules::build`.
//!
//! The generated root is currently a hand-written stub standing in for
//! `uic_codegen_web`; it exercises the exact layout the code generator emits
//! (`components/*.ts`, `components/_*.scss` partials, an `elements.scss`
//! aggregator compiled to `/elements.css`).

use std::fs;
use std::path::PathBuf;

use web_modules::build::{build, BuildOptions};
use web_modules::vendor::specs_from_package_json;

const HELLO_TS: &str = r#"// GENERATED stub — replaced by uic_codegen_web in milestone M3.
import { LitElement, html } from 'lit';

export class HelloUic extends LitElement {
  static tagName = 'hello-uic';

  static properties = {
    who: { type: String },
  };

  who = 'world';

  // Light DOM so Bootstrap's global stylesheet applies.
  createRenderRoot(): this {
    return this;
  }

  // ExternalStyles pattern: external stylesheets target .el-hello-uic.
  connectedCallback(): void {
    super.connectedCallback();
    this.classList.add('el-hello-uic');
  }

  render() {
    return html`<p class="badge text-bg-primary">Hello ${this.who} — cross-root import works.</p>`;
  }
}

customElements.define(HelloUic.tagName, HelloUic);
"#;

const HELLO_SCSS: &str = r#".el-hello-uic {
  display: block;

  p {
    font-style: italic;
  }
}
"#;

const ELEMENTS_SCSS: &str = r#"@use "components/hello-uic";
"#;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let web = manifest.join("web");

    let gen_web = out.join("gen_web");
    let components = gen_web.join("components");
    fs::create_dir_all(&components).expect("create generated components dir");
    fs::write(components.join("hello-uic.ts"), HELLO_TS).expect("write hello-uic.ts");
    fs::write(components.join("_hello-uic.scss"), HELLO_SCSS).expect("write _hello-uic.scss");
    fs::write(gen_web.join("elements.scss"), ELEMENTS_SCSS).expect("write elements.scss");

    // Browser deps come from web/package.json `dependencies` (import-map
    // entries auto-derived from each package.json).
    let specs = specs_from_package_json(&web.join("package.json"))
        .expect("read browser dependencies from web/package.json");

    build(&BuildOptions {
        specs: &specs,
        roots: &[web, gen_web],
        out: &out.join("dist"),
        mount: "/web_modules",
        html: "",
        template: None,
        processors: Default::default(),
        output: Default::default(),
    })
    .expect("build web-demo frontend");
}

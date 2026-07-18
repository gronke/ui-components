//! Vendors the exploration target (#65) through the in-house toolchain:
//! `@alenaksu/json-viewer` lands byte-unmodified in `$OUT_DIR/vendor/`, the
//! runtime includes its dist module from there.

use std::path::PathBuf;

use web_modules::vendor::{vendor, PackageSpec};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let vendor_dir = out.join("vendor");
    let dist = vendor_dir.join("@alenaksu/json-viewer/dist/json-viewer.js");
    let xterm = vendor_dir.join("@xterm/xterm/lib/xterm.js");

    let lit = vendor_dir.join("lit/index.js");

    // Network fetch only when absent: rebuilds stay offline.
    if !dist.is_file() || !xterm.is_file() || !lit.is_file() {
        vendor(
            &vendor_dir,
            "/vendor",
            &[
                PackageSpec::npm("@alenaksu/json-viewer", "^2").no_imports(),
                // The browser session example's terminal, the web-demo's pin.
                PackageSpec::npm("@xterm/xterm", "6.0.0").no_imports(),
                // The real lit family for the split view's DOM pane, the
                // web-demo's pins; the import map lives in the example page.
                PackageSpec::npm("lit", "^3").no_imports(),
                PackageSpec::npm("@lit/reactive-element", "^2").no_imports(),
                PackageSpec::npm("lit-html", "^3").no_imports(),
                PackageSpec::npm("lit-element", "^4").no_imports(),
            ],
        )
        .expect("vendor the exploration packages");
    }
    assert!(lit.is_file(), "vendored lit missing: {lit:?}");
    println!(
        "cargo:rustc-env=UIC_JS_VENDOR_ROOT={}",
        vendor_dir.display()
    );
    assert!(dist.is_file(), "vendored dist module missing: {dist:?}");
    assert!(xterm.is_file(), "vendored xterm missing: {xterm:?}");
    println!(
        "cargo:rustc-env=UIC_JS_VENDOR_DIST={}",
        dist.parent().expect("dist dir").display()
    );
    println!(
        "cargo:rustc-env=UIC_JS_VENDOR_XTERM={}",
        xterm
            .parent()
            .and_then(|p| p.parent())
            .expect("xterm dir")
            .display()
    );
}

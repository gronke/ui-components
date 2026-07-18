//! Vendors the packages declared in package.json — the compiled Bootstrap
//! the tui.css generator filters.

use std::path::PathBuf;

use web_modules::vendor::{specs_from_package_json, vendor};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let vendor_dir = out.join("vendor");
    let css = vendor_dir.join("bootstrap/dist/css/bootstrap.css");

    println!("cargo:rerun-if-changed=package.json");
    // Network fetch only when absent: rebuilds stay offline.
    if !css.is_file() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("package.json");
        let specs = specs_from_package_json(&manifest).expect("read package.json");
        vendor(&vendor_dir, "/vendor", &specs).expect("vendor bootstrap");
    }
    assert!(css.is_file(), "vendored bootstrap css missing: {css:?}");
    println!("cargo:rustc-env=UIC_CSS_BOOTSTRAP={}", css.display());
}

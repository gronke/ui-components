//! The component catalog: one Rust definition per custom element.
//!
//! Each component module co-locates its Rust definition (`date.rs`), its
//! lit-flavored template (`date.html`), its stylesheet (`date.scss`), and,
//! for behavior the browser cannot derive, its web partial (`date.impl.ts`).

pub mod connect;
pub mod input;
pub mod nav_breadcrumb;
pub mod nav_tabs;
pub mod tree;

pub use input::InputDate;
pub use nav_breadcrumb::NavBreadcrumb;
pub use nav_tabs::NavTabs;
pub use tree::Tree;

/// Anchors this crate's object code so `inventory` registrations survive the
/// linker in consuming binaries and build scripts. `inline(never)` keeps the
/// call a genuine symbol reference: cross-crate MIR inlining erases an empty
/// call in optimized non-incremental builds, and with no reference into this
/// crate's only object, wasm-ld's lazy archive extraction drops the
/// registration constructors and every component with them.
#[inline(never)]
pub fn link() {}

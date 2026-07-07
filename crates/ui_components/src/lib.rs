//! The component catalog: one Rust definition per custom element.
//!
//! Each component module co-locates its Rust definition (`date.rs`), its
//! lit-flavored template (`date.mhtml`), its stylesheet (`date.scss`), and —
//! for behavior the browser cannot derive — its web partial (`date.impl.ts`).

pub mod input;

pub use input::InputDate;

/// Anchors this crate's object code so `inventory` registrations survive the
/// linker in consuming binaries and build scripts.
pub fn link() {}

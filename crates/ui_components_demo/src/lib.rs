//! The catalog's demo composition (ADR 0013): `<app-root>` assembles the
//! input components around one `state` object, rather than being an input
//! itself. It ships out of the published npm tree (`dist = false`) yet rides
//! the generated web catalog and both runtimes, so it lives beside the
//! catalog rather than in it.

pub mod app_root;

pub use app_root::AppRoot;

/// Anchors this crate's object code so `inventory` keeps `<app-root>`'s
/// registration through the linker, the demo twin of `ui_components::link`.
/// A consumer that mounts or generates `app-root` calls this after
/// `ui_components::link()`.
#[inline(never)]
pub fn link() {}

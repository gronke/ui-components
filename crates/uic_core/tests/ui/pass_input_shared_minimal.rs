// Doubles as the span probe: the derive resolves _shared/chrome.mhtml
// relative to THIS file even though it expands input_shared's re-emitted
// struct.
use uic_core::{input_shared, CustomElement};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(tag = "x-shared", template = "<input data-tui=\"text-input\">")]
struct XShared {}

impl XSharedLogic for XShared {}

fn main() {
    let def = XShared::definition();
    assert_eq!(def.shared_style_id, Some("input-default"));
    assert!(def.property("label").is_some());
    assert!(def.property("required").is_some());
    assert!(def.wraps_src.is_some());
}

use uic_core::{input_shared, CustomElement};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-conflict",
    template = "<p>x</p>",
    wraps_file = "_shared/chrome.html"
)]
struct XConflict {}

fn main() {}

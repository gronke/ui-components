use uic_core::{input_shared, CustomElement};

#[derive(CustomElement, Default)]
#[input_shared]
#[custom_element(tag = "x-below", template = "<p>x</p>")]
struct XBelow {}

fn main() {}

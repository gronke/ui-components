use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-bad", template = "<p>${a.b}</p>")]
struct XBad {}

impl XBadLogic for XBad {}

fn main() {}

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "box", template = "<p>x</p>")]
struct XBox {}

fn main() {}

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(template = "<p>x</p>")]
struct XNoTag {}

fn main() {}

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-two", template = "<p>x</p>", template_file = "x.html")]
struct XTwo {}

fn main() {}

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-btn", template = "<button @click=${on_click}>x</button>")]
struct XBtn {}

fn main() {}

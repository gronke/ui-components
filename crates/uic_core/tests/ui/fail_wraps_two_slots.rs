use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-twoslots",
    template = "<p>x</p>",
    wraps_file = "chrome_two_slots.html"
)]
struct XTwoSlots {}

fn main() {}

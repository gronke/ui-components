use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-noslot",
    template = "<p>x</p>",
    wraps_file = "chrome_no_slot.mhtml"
)]
struct XNoSlot {}

fn main() {}

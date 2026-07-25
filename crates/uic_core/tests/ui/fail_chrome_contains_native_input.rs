use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-chromeinput",
    template = "<p>x</p>",
    wraps_file = "chrome_with_native_input.html"
)]
struct XChromeInput {}

fn main() {}

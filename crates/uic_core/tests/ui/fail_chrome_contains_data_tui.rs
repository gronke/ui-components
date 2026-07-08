use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-chromewidget",
    template = "<p>x</p>",
    wraps_file = "chrome_with_widget.mhtml"
)]
struct XChromeWidget {}

fn main() {}

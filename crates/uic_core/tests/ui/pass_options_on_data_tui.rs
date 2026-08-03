use uic_core::{CustomElement, SelectOption};

// A data-tui widget receives its `.options` rows through the adapter
// (ADR 0002); the binding is legal beyond <select> and custom elements.
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-options-on-widget",
    template = "<input data-tui=\"x-widget\" .options=${options} />"
)]
struct XOptionsOnWidget {
    #[property]
    options: Vec<SelectOption>,
}

impl XOptionsOnWidgetLogic for XOptionsOnWidget {}

fn main() {
    assert_eq!(XOptionsOnWidget::definition().tag_name, "x-options-on-widget");
}

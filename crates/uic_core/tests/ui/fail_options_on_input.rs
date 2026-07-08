use uic_core::{CustomElement, SelectOption};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-options-on-input",
    template = "<input .options=${options} />"
)]
struct XOptionsOnInput {
    #[property]
    options: Vec<SelectOption>,
}

fn main() {}

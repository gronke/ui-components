use uic_core::{CustomElement, SelectOption};

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-options-optional", template = "<p>x</p>")]
struct XOptionsOptional {
    #[property]
    options: Option<Vec<SelectOption>>,
}

fn main() {}

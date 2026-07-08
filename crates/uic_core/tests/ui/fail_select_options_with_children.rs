use uic_core::{CustomElement, SelectOption};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-select-children",
    template = "<select .options=${options}><option>static</option></select>"
)]
struct XSelectChildren {
    #[property]
    options: Vec<SelectOption>,
}

fn main() {}

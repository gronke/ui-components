use uic_core::{CustomElement, SelectOption};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "x-forinput",
    template = "<div><template for=${rows} as=r><input type=\"text\"></template></div>"
)]
struct XForInput {
    #[property]
    rows: Vec<SelectOption>,
}

fn main() {}

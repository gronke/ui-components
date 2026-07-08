use uic_core::{input_shared, CustomElement};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(tag = "x-dupe", template = "<p>${label}</p>")]
struct XDupe {
    #[property]
    label: Option<String>,
}

fn main() {}

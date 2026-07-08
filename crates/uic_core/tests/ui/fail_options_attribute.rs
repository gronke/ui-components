use uic_core::{CustomElement, SelectOption};

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-options-reflect", template = "<p>x</p>")]
struct XOptionsReflect {
    #[property(reflect)]
    options: Vec<SelectOption>,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-options-attr", template = "<p>x</p>")]
struct XOptionsAttr {
    #[property(attribute = "options")]
    options: Vec<SelectOption>,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-options-default", template = "<p>x</p>")]
struct XOptionsDefault {
    #[property(default = "a,b")]
    options: Vec<SelectOption>,
}

fn main() {}

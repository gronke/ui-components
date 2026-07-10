use uic_core::{CustomElement, ObjectMap};

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-object-optional", template = "<p>x</p>")]
struct XObjectOptional {
    #[property]
    state: Option<ObjectMap>,
}

fn main() {}

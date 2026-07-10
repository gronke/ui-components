use uic_core::{CustomElement, ObjectMap};

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-object-reflect", template = "<p>x</p>")]
struct XObjectReflect {
    #[property(reflect)]
    state: ObjectMap,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-object-attr", template = "<p>x</p>")]
struct XObjectAttr {
    #[property(attribute = "state")]
    state: ObjectMap,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-object-default", template = "<p>x</p>")]
struct XObjectDefault {
    #[property(default = "x")]
    state: ObjectMap,
}

fn main() {}

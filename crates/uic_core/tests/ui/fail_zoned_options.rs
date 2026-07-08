use uic_core::{CustomElement, Zoned};

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-zoned-reflect", template = "<p>x</p>")]
struct XZonedReflect {
    #[property(reflect)]
    date: Option<Zoned>,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-zoned-attr", template = "<p>x</p>")]
struct XZonedAttr {
    #[property(attribute = "date")]
    date: Option<Zoned>,
}

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-zoned-default", template = "<p>x</p>")]
struct XZonedDefault {
    #[property(default = "2026-07-07")]
    date: Option<Zoned>,
}

fn main() {}

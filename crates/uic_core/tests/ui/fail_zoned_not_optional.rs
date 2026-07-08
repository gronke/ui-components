use uic_core::{CustomElement, Zoned};

// No derive(Default): a bare Zoned field has no default, which is exactly
// why the derive insists on Option<Zoned>.
#[derive(CustomElement)]
#[custom_element(tag = "x-zoned", template = "<p>x</p>")]
struct XZoned {
    #[property(notify)]
    date: Zoned,
}

fn main() {}

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-mismatch", template = "<p>x</p>")]
struct XMismatch {
    #[property(default = "x")]
    count: f64,
}

fn main() {}

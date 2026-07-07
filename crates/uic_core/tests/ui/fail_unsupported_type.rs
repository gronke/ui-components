use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-vec", template = "<p>x</p>")]
struct XVec {
    #[property]
    items: Vec<String>,
}

fn main() {}

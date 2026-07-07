use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(tag = "x-static", template = "<p>static</p>")]
struct XStatic {}

impl XStaticLogic for XStatic {}

fn main() {
    assert_eq!(XStatic::definition().tag_name, "x-static");
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_minimal.rs");
    t.compile_fail("tests/ui/fail_missing_logic_impl.rs");
    t.compile_fail("tests/ui/fail_bad_template.rs");
    t.compile_fail("tests/ui/fail_missing_tag.rs");
    t.compile_fail("tests/ui/fail_both_templates.rs");
    t.compile_fail("tests/ui/fail_unsupported_type.rs");
    t.compile_fail("tests/ui/fail_tag_without_dash.rs");
}

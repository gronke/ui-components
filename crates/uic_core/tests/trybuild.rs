#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_minimal.rs");
    t.pass("tests/ui/pass_input_shared_minimal.rs");
    t.compile_fail("tests/ui/fail_input_shared_below_derive.rs");
    t.compile_fail("tests/ui/fail_input_shared_duplicate_field.rs");
    t.compile_fail("tests/ui/fail_input_shared_wraps_conflict.rs");
    t.compile_fail("tests/ui/fail_missing_logic_impl.rs");
    t.compile_fail("tests/ui/fail_bad_template.rs");
    t.compile_fail("tests/ui/fail_missing_tag.rs");
    t.compile_fail("tests/ui/fail_both_templates.rs");
    t.compile_fail("tests/ui/fail_unsupported_type.rs");
    t.compile_fail("tests/ui/fail_zoned_not_optional.rs");
    t.compile_fail("tests/ui/fail_zoned_options.rs");
    t.compile_fail("tests/ui/fail_options_optional.rs");
    t.compile_fail("tests/ui/fail_options_attribute.rs");
    t.compile_fail("tests/ui/fail_object_optional.rs");
    t.compile_fail("tests/ui/fail_object_attribute.rs");
    t.compile_fail("tests/ui/fail_options_on_input.rs");
    t.compile_fail("tests/ui/fail_select_options_with_children.rs");
    t.compile_fail("tests/ui/fail_tag_without_dash.rs");
    t.compile_fail("tests/ui/fail_default_type_mismatch.rs");
    t.compile_fail("tests/ui/fail_wraps_no_slot.rs");
    t.compile_fail("tests/ui/fail_wraps_two_slots.rs");
    t.compile_fail("tests/ui/fail_chrome_contains_data_tui.rs");
}

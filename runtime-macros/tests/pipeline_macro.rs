// trybuild-driven contract tests for `#[topo_pipeline]`.
//
// `pass_*` cases assert the supported surface compiles; `fail_*`
// cases assert that drift outside the supported surface produces a
// compile_error pointing at the offending input (silent drop is the
// failure mode the macro contract closes).

#[test]
fn pipeline_macro_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/cases/pass_basic.rs");
    t.pass("tests/cases/pass_void.rs");
    t.pass("tests/cases/pass_generic.rs");
    t.pass("tests/cases/pass_empty_parens.rs");
    t.compile_fail("tests/cases/fail_non_empty_body.rs");
    t.compile_fail("tests/cases/fail_unknown_attr.rs");
}

#[test]
fn compile_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/macro/pass/*.rs");
    t.compile_fail("tests/macro/fail/*.rs");
}

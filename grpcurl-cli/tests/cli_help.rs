mod common;

use common::{assert_exit_code, assert_output_contains, run};

#[test]
fn help_flag() {
    let r = run(&["-help"]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "[address]");
}

#[test]
fn version_flag() {
    let r = run(&["-version"]);
    assert_exit_code(&r, 0);
}

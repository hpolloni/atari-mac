//! The helper of the planned test stubs (`docs/design/spec-gates.md`, "Planned test stubs").

/// Fails the test: the test is a planned stub that a package replaces with the real test.
#[expect(
    clippy::redundant_pub_crate,
    reason = "the stubs in all test modules of the crate call the helper"
)]
pub(crate) fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

//! The body of a planned test stub (`docs/design/spec-gates.md`, "Planned test stubs").

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(
    clippy::redundant_pub_crate,
    reason = "the test modules of all chips use the helper"
)]
pub(crate) fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

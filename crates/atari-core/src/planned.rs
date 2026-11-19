//! The helper for planned test stubs (`docs/design/spec-gates.md`, "Planned test stubs").

/// Fails the calling test: a planned stub never counts as a pass.
#[expect(
    clippy::redundant_pub_crate,
    reason = "`unreachable_pub` forbids `pub` in this private module"
)]
pub(crate) fn planned() {
    panic!("planned test: not written yet");
}

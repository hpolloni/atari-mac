//! The helper of the planned test stubs (REQ-027).

/// Fails the planned test stub that calls it: the test is not written yet.
#[expect(
    clippy::redundant_pub_crate,
    reason = "`unreachable_pub` rejects `pub` in this private test module"
)]
pub(crate) fn planned() {
    panic!("planned test: not written yet");
}

//! Paddle values in BASIC (REQ-534).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

/// Fails the planned test stub that calls it: the test is not written yet (REQ-027).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: not written yet");
}

/// Verifies REQ-534: System check: with a host paddle value on input 0, the BASIC `PADDLE(0)`
/// function returns that value after the OS vertical blank scan (`POTGO`, `POT0`).
#[test]
#[ignore = "planned: SLICE-04"]
fn basic_paddle_reads_host_value() {
    planned();
}

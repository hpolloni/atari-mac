//! POKEY serial input with the OS SIO routines (REQ-526).
//!
//! Each test is a stub until its package writes it (`docs/design/pokey.md`, "Test plan").

/// Fails the planned test stub that calls it: the test is not written yet (REQ-027).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: not written yet");
}

/// Verifies REQ-526: System check: the OS SIO routine receives a frame from a device on SIO DATA
/// IN; each byte reaches `SERIN` with the serial input IRQ (`IRQST` bit 5), and the OS gets the
/// correct data. The test needs the SIO bus model of REQ-600 and later.
#[test]
#[ignore = "planned: SLICE-05"]
fn os_sio_receives_frame() {
    planned();
}

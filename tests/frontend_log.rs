//! The records of the category `frontend` (REQ-085 criterion 3), with a capture logger.
//!
//! Design: `docs/design/architecture.md`, "Frontend records". The logger and the maximum
//! level of the `log` crate are global to the process, so these tests are in their own test
//! binary.

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-085: with a capture logger at `Info`, the `atari::frontend` records are
/// `turbo on (BASIC typing)` at the first frame of a `--bas` typist and `turbo off` after
/// the typing, `turbo on (tape motor)` and `turbo off` with the motor line of a tape,
/// `paused` and `continued` at F9, and `cold reset (F10)` at F10. Each record has the frame
/// count and the cycle count of the machine at the start of its frame (criteria 2 and 3).
#[test]
#[ignore = "planned: SLICE-09"]
fn frontend_records() {
    planned();
}

//! Robustness tests: random media files and random programs (REQ-016).

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-016: with a fixed seed, 1,000 random byte arrays of 0 to 70,000 bytes load
/// as each media type with no panic (criterion 2).
#[test]
#[ignore = "planned: SLICE-04"]
fn random_files_do_not_panic() {
    planned();
}

/// Verifies REQ-016: with a fixed seed, random bytes in RAM and `PC` = `$0600` run for 600
/// frames with no panic (criterion 3).
#[test]
#[ignore = "planned: SLICE-04"]
fn random_programs_do_not_panic() {
    planned();
}

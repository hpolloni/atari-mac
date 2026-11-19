//! Golden frame tests: frame comparisons with reference PNG files (REQ-026).

/// Fails the test that calls it. A planned stub calls this function, so that the stub never
/// passes when it runs (`cargo test -- --include-ignored`).
#[expect(clippy::panic, reason = "a planned test stub must fail when it runs")]
fn planned() {
    panic!("planned test: a package replaces this stub with the real test");
}

/// Verifies REQ-026: the boot screen passes only when every pixel is equal to the reference
/// PNG (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn boot_screen_matches_golden() {
    planned();
}

/// Verifies REQ-026: on a difference, the comparison fails and writes
/// `target/golden-failures/<name>-actual.png` and `<name>-diff.png` (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn difference_writes_failure_images() {
    planned();
}

/// Verifies REQ-026: with `UPDATE_GOLDEN=1`, the comparison writes the reference PNG and
/// passes (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn update_mode_writes_reference() {
    planned();
}

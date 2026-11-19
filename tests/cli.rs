//! End-to-end tests for the `atari-mac` binary.

use std::process::Command;

#[test]
fn binary_prints_greeting() {
    let output = Command::new(env!("CARGO_BIN_EXE_atari-mac"))
        .output()
        .expect("failed to run the atari-mac binary");

    assert!(
        output.status.success(),
        "binary exited with {}",
        output.status
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", atari_mac::greeting()),
        "unexpected stdout",
    );
    assert!(output.stderr.is_empty(), "unexpected stderr output");
}

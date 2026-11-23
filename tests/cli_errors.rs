//! The output of the `atari-mac` binary for a command line that is not valid (REQ-096,
//! REQ-097).
//!
//! Design: `docs/design/architecture.md`, "Command-line errors (REQ-096, REQ-097)". The unit
//! tests of `src/cli.rs` check each message; these tests check what the binary writes.

use std::process::Command;

// The binary uses these dependencies; this test does not.
use atari_mac as _;
use minifb as _;

/// Runs the binary with `args` and checks that it exits with code 2, writes nothing to
/// standard output, and writes exactly the line `atari-mac: <message> (see --help)` to
/// standard error.
fn assert_usage_output(args: &[&str], message: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_atari-mac"))
        .args(args)
        .output()
        .expect("failed to run the atari-mac binary");
    assert_eq!(output.status.code(), Some(2), "exit code of {args:?}");
    assert!(output.stdout.is_empty(), "standard output of {args:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    let line = format!("atari-mac: {message} (see --help)");
    assert_eq!(lines, [line.as_str()], "standard error of {args:?}");
    assert!(stderr.ends_with('\n'), "one full line for {args:?}");
}

/// Verifies REQ-096 criterion 5: for each kind of syntax error, the binary exits with code
/// 2, writes one line with the prefix `atari-mac: ` and the suffix ` (see --help)` to standard
/// error, and writes nothing to standard output.
#[test]
fn syntax_error_output() {
    for (args, message) in [
        (&["--fast"][..], "unknown argument \"--fast\""),
        (&["--os=file"], "unknown argument \"--os=file\""),
        (&[""], "unknown argument \"\""),
        (
            &["--headless", "--frames", "1", "--os"],
            "--os needs a value",
        ),
        (&["--frames", "x"], "--frames needs a number, not \"x\""),
        (&["--scale", "9"], "--scale must be 1 to 8, not 9"),
    ] {
        assert_usage_output(args, message);
    }
}

/// Verifies REQ-097 criterion 7: a combination error has the output of a syntax error,
/// and a syntax error comes before it.
#[test]
fn combination_error_output() {
    for (args, message) in [
        (&["--headless"][..], "--headless needs --frames"),
        (
            &["--xex", "a.xex", "--disk", "d.atr"],
            "--xex and --disk cannot be used together: both use drive D1:",
        ),
        (
            &["--trace-frames", "0"],
            "--trace-frames must be at least 1",
        ),
        (
            &["--trace-from", "1"],
            "--trace-from and --trace-frames need --trace",
        ),
        (&["--headless", "--fast"], "unknown argument \"--fast\""),
        (
            &["--xex", "a.xex", "--disk", "d.atr", "--scale", "0"],
            "--scale must be 1 to 8, not 0",
        ),
    ] {
        assert_usage_output(args, message);
    }
}

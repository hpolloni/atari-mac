//! Typing on the emulated keyboard (REQ-025).

use atari_harness::HarnessError;
use atari_harness::input::type_text;
use atari_harness::screen::{has_row, screen_text};
use atari_harness::system::{BootOptions, boot, run_frames, run_until};

/// Verifies REQ-025: `PRINT 2+2` in BASIC shows `4`, then `READY`.
#[test]
#[ignore = "needs WP-1, WP-2, WP-3: REQ-025"]
fn basic_prints_sum() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let ready = run_until(&mut machine, 180, |m| has_row(m, "READY"));
    assert!(ready.is_some(), "no READY row");
    type_text(&mut machine, "PRINT 2+2\n").unwrap();
    run_frames(&mut machine, 60);
    let rows: Vec<String> = screen_text(&machine)
        .iter()
        .map(|r| r.trim_end().to_owned())
        .collect();
    let four = rows.iter().rposition(|row| row == "4");
    assert!(four.is_some(), "no row `4`: {rows:#?}");
    assert_eq!(
        rows.get(four.unwrap_or(0) + 1).map(String::as_str),
        Some("READY"),
        "{rows:#?}"
    );
}

/// Verifies REQ-025: a character without a key gives `NoKey`.
#[test]
fn type_text_rejects_unknown_character() {
    let mut machine = boot(BootOptions::default()).unwrap();
    assert_eq!(
        type_text(&mut machine, "€"),
        Err(HarnessError::NoKey('€')),
        "euro sign"
    );
}

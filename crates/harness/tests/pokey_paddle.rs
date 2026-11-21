//! Paddle values in BASIC (REQ-534).
//!
//! The OS vertical blank copies `POT0-POT7` to the shadow registers `PADDL0-PADDL7` and writes
//! `POTGO` for the next scan. The BASIC `PADDLE` function reads the shadow registers.

use atari_core::Machine;
use atari_harness::input::type_text;
use atari_harness::screen::{has_row, screen_text};
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_frames, run_until};

/// The OS shadow of `POT0` (`PADDL0`).
const PADDL0: u16 = 0x0270;

/// The screen rows without the margin and the trailing spaces.
fn rows(machine: &Machine) -> Vec<String> {
    screen_text(machine)
        .iter()
        .map(|row| row.trim().to_owned())
        .collect()
}

/// Verifies REQ-534: System check: with a host paddle value on input 0, the BASIC `PADDLE(0)`
/// function returns that value after the OS vertical blank scan (`POTGO`, `POT0`).
#[test]
fn basic_paddle_reads_host_value() {
    let mut machine = boot(BootOptions::default()).unwrap();
    let ready = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, "Ready"));
    assert!(ready.is_some(), "no Ready row after {BOOT_FRAMES} frames");
    // A host count of 255 is no paddle, which reads 228 (REQ-535 criterion 2).
    for (host, count) in [(1, 1), (100, 100), (227, 227), (255, 228)] {
        machine.set_paddle(0, host);
        // One scan takes 228 scanlines, so the shadow has the value after 2 frames.
        run_frames(&mut machine, 3);
        assert_eq!(machine.peek(PADDL0), count, "PADDL0");
        type_text(&mut machine, "PRINT PADDLE(0)\n").unwrap();
        run_frames(&mut machine, 60);
        let rows = rows(&machine);
        let expected = count.to_string();
        let printed = rows.iter().rposition(|row| *row == expected);
        assert!(printed.is_some(), "no row `{expected}`: {rows:#?}");
        let command = rows.iter().rposition(|row| row == "PRINT PADDLE(0)");
        assert!(
            command.is_some_and(|c| Some(c + 1) == printed),
            "the value follows the command: {rows:#?}"
        );
    }
}

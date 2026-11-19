//! Typing on the emulated keyboard (REQ-025, REQ-518, REQ-522).

use atari_core::Machine;
use atari_harness::HarnessError;
use atari_harness::input::type_text;
use atari_harness::screen::{has_row, screen_text};
use atari_harness::system::{BOOT_FRAMES, BootOptions, boot, run_frames, run_until};

/// The prompt of Altirra BASIC.
const READY: &str = "Ready";

/// A machine after power-on, and the frames until the BASIC prompt shows (`None` if it
/// does not show).
fn boot_to_ready() -> Result<(Machine, Option<u32>), HarnessError> {
    let mut machine = boot(BootOptions::default())?;
    let ready = run_until(&mut machine, BOOT_FRAMES, |m| has_row(m, READY));
    Ok((machine, ready))
}

/// The first row after row `index` that is not empty. BASIC prints an empty row before its
/// prompt.
fn next_text_row(rows: &[String], index: usize) -> Option<&str> {
    rows.iter()
        .skip(index + 1)
        .find(|row| !row.is_empty())
        .map(String::as_str)
}

/// The screen rows without the margin and the trailing spaces.
fn rows(machine: &Machine) -> Vec<String> {
    screen_text(machine)
        .iter()
        .map(|row| row.trim().to_owned())
        .collect()
}

/// Verifies REQ-025 and REQ-518: `PRINT 2+2` in BASIC shows `4`, then the prompt.
#[test]
fn basic_prints_sum() {
    let (mut machine, ready) = boot_to_ready().unwrap();
    assert!(ready.is_some(), "no {READY} row");
    type_text(&mut machine, "PRINT 2+2\n").unwrap();
    run_frames(&mut machine, 60);
    let rows = rows(&machine);
    let four = rows.iter().rposition(|row| row == "4");
    assert!(four.is_some(), "no row `4`: {rows:#?}");
    assert_eq!(
        next_text_row(&rows, four.unwrap_or(0)),
        Some(READY),
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

/// Verifies REQ-522: the BREAK key stops a running BASIC program.
///
/// Altirra BASIC prints `Stopped at line 10` and then waits for a command without a new
/// prompt. The test types a command to show that the program does not run any more.
#[test]
fn break_stops_basic_program() {
    let (mut machine, ready) = boot_to_ready().unwrap();
    assert!(ready.is_some(), "no {READY} row");
    type_text(&mut machine, "10 GOTO 10\nRUN\n").unwrap();
    run_frames(&mut machine, 30);
    let running = rows(&machine);
    assert!(
        !running.iter().any(|row| row.starts_with("Stopped")),
        "stopped before BREAK: {running:#?}"
    );
    machine.set_break(true);
    run_frames(&mut machine, 3);
    machine.set_break(false);
    run_frames(&mut machine, 30);
    let stopped = rows(&machine);
    assert!(
        stopped.iter().any(|row| row == "Stopped at line 10"),
        "no row `Stopped at line 10`: {stopped:#?}"
    );
    type_text(&mut machine, "PRINT 2+2\n").unwrap();
    run_frames(&mut machine, 60);
    let rows = rows(&machine);
    let four = rows.iter().rposition(|row| row == "4");
    assert!(four.is_some(), "no row `4` after BREAK: {rows:#?}");
    assert_eq!(
        next_text_row(&rows, four.unwrap_or(0)),
        Some(READY),
        "{rows:#?}"
    );
}

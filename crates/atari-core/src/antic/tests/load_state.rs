//! Tests of the check of a loaded ANTIC state (REQ-061).
//!
//! Each test sets one counter of a new chip exactly at its limit (rejected) and one step
//! inside it (accepted), and calls `loaded_is_valid` directly.

use super::Antic;
use crate::state::TIME_LIMIT;
use crate::timing::{CYCLES_PER_LINE, VideoStandard};

/// `loaded_is_valid` of a new NTSC chip after `change`, against a new NTSC chip as the
/// state before the load.
fn valid_after(change: impl FnOnce(&mut Antic)) -> bool {
    let config = Antic::new(VideoStandard::Ntsc);
    let mut antic = config;
    change(&mut antic);
    antic.loaded_is_valid(&config)
}

/// Verifies REQ-061: a new chip is a valid loaded state, and a state of the other TV
/// standard is not.
#[test]
fn antic_load_check_standard() {
    assert!(valid_after(|_| {}), "new chip");
    assert!(
        !valid_after(|a| a.standard = VideoStandard::Pal),
        "other standard"
    );
}

/// Verifies REQ-061: the scan line counters must be below the lines of the frame.
#[test]
fn antic_load_check_lines() {
    let lines = VideoStandard::Ntsc.lines_per_frame();
    assert!(valid_after(|a| a.line = lines - 1), "line: last");
    assert!(!valid_after(|a| a.line = lines), "line: limit");
    assert!(valid_after(|a| a.cur_line = lines - 1), "cur_line: last");
    assert!(!valid_after(|a| a.cur_line = lines), "cur_line: limit");
}

/// Verifies REQ-061: the cycle counters must be below the cycles of a line.
#[test]
fn antic_load_check_cycles() {
    assert!(
        valid_after(|a| a.cycle = CYCLES_PER_LINE - 1),
        "cycle: last"
    );
    assert!(!valid_after(|a| a.cycle = CYCLES_PER_LINE), "cycle: limit");
    assert!(
        valid_after(|a| a.cur_cycle = CYCLES_PER_LINE - 1),
        "cur_cycle: last"
    );
    assert!(
        !valid_after(|a| a.cur_cycle = CYCLES_PER_LINE),
        "cur_cycle: limit"
    );
}

/// Verifies REQ-061: the cycle count must be below `TIME_LIMIT`.
#[test]
fn antic_load_check_time_limit() {
    assert!(
        valid_after(|a| a.total_cycles = TIME_LIMIT - 1),
        "below the limit"
    );
    assert!(
        !valid_after(|a| a.total_cycles = TIME_LIMIT),
        "at the limit"
    );
}

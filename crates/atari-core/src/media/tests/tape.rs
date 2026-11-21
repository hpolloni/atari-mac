//! Planned tests of the cassette deck: the motor and the tape signal
//! (`docs/design/media.md`, "Test method for the cassette deck").

use crate::planned::planned;

/// Verifies REQ-641: with a tape inserted and the motor line high, `level` is `true` on
/// each of 100,000 cycles, and the position stays 0 (criterion 1).
#[test]
#[ignore = "planned: SLICE-08"]
fn req641_motor_off_no_motion() {
    planned();
}

/// Verifies REQ-641: after `set_motor_line(true, M)`, `level(M + 1 + p)` equals
/// `signal_at(p)` for p from 0 to 200,000 (criterion 2).
#[test]
#[ignore = "planned: SLICE-08"]
fn req641_motor_start() {
    planned();
}

/// Verifies REQ-641: after `set_motor_line(false, H)`, `level` is `true` from H + 1 and the
/// position is H - M; after `set_motor_line(true, M2)`, `level(M2 + 1 + k)` equals
/// `signal_at(H - M + k)` for k from 0 to 200,000 (criterion 3).
#[test]
#[ignore = "planned: SLICE-08"]
fn req641_motor_stop_keeps_position() {
    planned();
}

/// Verifies REQ-641: after `eject`, `level` is `true` on each cycle, also with the motor
/// line low (criterion 6).
#[test]
#[ignore = "planned: SLICE-08"]
fn req641_eject() {
    planned();
}

/// Verifies REQ-642: NTSC, tape A (gap 100 ms, 600 baud, `55 A5`): the gap ends at 178,976,
/// bit 1 starts at 181,960, the stop bit of `$55` is 205,824-208,806, `$A5` starts at
/// 208,807, the block ends at 238,636, the 20 bit levels are `0101010101` and
/// `0101001011`, and the line is 1 after the block; block 1 of file T ends at 39,732,950
/// (criteria 1, 2, 5).
#[test]
#[ignore = "planned: SLICE-08"]
fn req642_data_block_ntsc() {
    planned();
}

/// Verifies REQ-642: PAL, tape A: the gap ends at 177,344, bit 1 starts at 180,301, `$A5`
/// starts at 206,902, the block ends at 236,460, and the bit levels are those of NTSC
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-08"]
fn req642_data_block_pal() {
    planned();
}

/// Verifies REQ-642: tape B (lengths 10 and 20): NTSC 0 for 0-1,789, 1 for 1,790-5,368 and
/// after; PAL 0 for 0-1,772, then 1 (criterion 4).
#[test]
#[ignore = "planned: SLICE-08"]
fn req642_signal_block() {
    planned();
}

/// Verifies REQ-642: `SioBus::data_in_level` is the logical AND of the tape level and the
/// disk drive line: a drive stop bit during a tape 0 gives 0, a drive start bit during a
/// tape gap gives 0 (criterion 6).
#[test]
#[ignore = "planned: SLICE-08"]
fn req642_and_with_drive() {
    planned();
}

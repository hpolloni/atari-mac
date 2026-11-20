//! The text of the OS text screen, read from the screen memory (REQ-024).
//!
//! The functions read memory only (with [`Machine::peek`]), so they do not depend on ANTIC
//! or GTIA. The test harness (`atari_harness::screen`) and the frontend (`--bas`, REQ-080)
//! both use them.
//!
//! Design: `docs/design/test-harness.md`, "Module `screen`".

use crate::Machine;

/// Rows of the OS text screen (GRAPHICS 0).
pub const ROWS: usize = 24;

/// Columns of the OS text screen (GRAPHICS 0).
pub const COLUMNS: usize = 40;

/// The OS variable `SAVMSC`: the address of the screen memory.
pub const SAVMSC: u16 = 0x58;

/// Converts one ANTIC internal character code to ASCII.
///
/// Bit 7 (inverse video) is ignored. Codes `$00-$3F` are ATASCII `$20-$5F`, codes `$40-$5F`
/// are ATASCII `$00-$1F` (graphics characters, shown as `?`), and codes `$60-$7F` are
/// ATASCII `$60-$7F`.
#[must_use]
pub fn internal_to_ascii(code: u8) -> char {
    let atascii = match code & 0x7F {
        c @ 0x00..=0x3F => c + 0x20,
        c @ 0x40..=0x5F => c - 0x40,
        c => c,
    };
    if (0x20..0x7F).contains(&atascii) {
        char::from(atascii)
    } else {
        '?'
    }
}

/// The 24 rows of 40 characters at the address in `SAVMSC`. This reads memory only, so it
/// does not depend on ANTIC or GTIA.
#[must_use]
pub fn screen_text(machine: &Machine) -> Vec<String> {
    let base = u16::from_le_bytes([machine.peek(SAVMSC), machine.peek(SAVMSC + 1)]);
    (0..ROWS)
        .map(|row| {
            (0..COLUMNS)
                .map(|column| {
                    let offset = u16::try_from(row * COLUMNS + column).unwrap_or(0);
                    internal_to_ascii(machine.peek(base.wrapping_add(offset)))
                })
                .collect()
        })
        .collect()
}

/// `true` if a row of the screen, with leading and trailing spaces removed, equals `text`.
///
/// The OS screen editor starts each line at the left margin (`LMARGN`, 2 after power-on), so
/// the text of a row starts after the margin spaces.
#[must_use]
pub fn has_row(machine: &Machine, text: &str) -> bool {
    screen_text(machine).iter().any(|row| row.trim() == text)
}

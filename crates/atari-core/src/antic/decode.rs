//! Mode tables and the playfield decoder: bytes to ANTIC-to-GTIA codes.
//!
//! Design: `docs/design/antic.md`, "Character modes", "Mapped modes" and "Pixel output".

use super::AnCode;

/// The codes that one playfield byte makes, left first. Mode 8 makes the most (16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Codes {
    /// The codes. Only the first `len` are valid.
    pub(super) codes: [AnCode; 16],
    /// The number of color clocks.
    pub(super) len: usize,
}

impl Codes {
    /// An empty list.
    const fn new() -> Self {
        Self {
            codes: [AnCode::BACKGROUND; 16],
            len: 0,
        }
    }

    /// Adds `code` `count` times.
    fn push(&mut self, code: AnCode, count: usize) {
        for _ in 0..count {
            if let Some(slot) = self.codes.get_mut(self.len) {
                *slot = code;
                self.len += 1;
            }
        }
    }

    /// The valid codes.
    pub(super) fn as_slice(&self) -> &[AnCode] {
        self.codes.get(..self.len).unwrap_or_default()
    }
}

/// The IR mode (bits 3-0).
pub(super) const fn mode(ir: u8) -> u8 {
    ir & 0x0F
}

/// The normal height of a mode line in scan lines (REQ-312, REQ-320, REQ-321).
pub(super) const fn height(ir: u8) -> u8 {
    match mode(ir) {
        0 => ((ir >> 4) & 7) + 1,
        3 => 10,
        5 | 7 => 16,
        2 | 4 | 6 | 8 => 8,
        9 | 0x0A => 4,
        0x0B | 0x0D => 2,
        // 1 (jump), C, E and F.
        _ => 1,
    }
}

/// The playfield DMA clock period in cycles (HRM 4.12 Table 7).
pub(super) const fn period(mode: u8) -> u32 {
    match mode {
        6 | 7 | 0x0A..=0x0C => 4,
        8 | 9 => 8,
        _ => 2,
    }
}

/// Modes 2, 3 and F are high resolution modes (HRM 6.8).
pub(super) const fn is_hires(mode: u8) -> bool {
    matches!(mode, 2 | 3 | 0x0F)
}

/// Modes 2-7 are character modes.
pub(super) const fn is_char(mode: u8) -> bool {
    matches!(mode, 2..=7)
}

/// The code of a 2-bit pixel in modes 4, 5, 8, A, D and E. `pf3` selects `PF3` for `%11`.
const fn two_bit(pixel: u8, pf3: bool) -> AnCode {
    match pixel & 3 {
        0 => AnCode::BACKGROUND,
        3 if pf3 => AnCode::playfield(3),
        p => AnCode::playfield(p - 1),
    }
}

/// Four hi-res pixel pairs of one byte (modes 2, 3 and F, REQ-307).
fn hires_codes(data: u8) -> Codes {
    let mut out = Codes::new();
    for shift in [6, 4, 2, 0] {
        out.push(AnCode::hires(data >> shift), 1);
    }
    out
}

/// Four 2-bit pixels of one byte, each `width` color clocks wide.
fn two_bit_codes(data: u8, width: usize, pf3: bool) -> Codes {
    let mut out = Codes::new();
    for shift in [6, 4, 2, 0] {
        out.push(two_bit(data >> shift, pf3), width);
    }
    out
}

/// Eight 1-bit pixels of one byte, each `width` color clocks wide. A 1 bit gives `on`.
fn one_bit_codes(data: u8, width: usize, on: AnCode) -> Codes {
    let mut out = Codes::new();
    for bit in (0..8).rev() {
        let code = if (data >> bit) & 1 == 1 {
            on
        } else {
            AnCode::BACKGROUND
        };
        out.push(code, width);
    }
    out
}

/// The codes of one character data byte (REQ-313, REQ-315, REQ-316). `data` already has
/// the `CHACTL` blink and invert applied.
pub(super) fn char_codes(mode: u8, name: u8, data: u8) -> Codes {
    match mode {
        4 | 5 => two_bit_codes(data, 1, name & 0x80 != 0),
        6 | 7 => one_bit_codes(data, 1, AnCode::playfield(name >> 6)),
        _ => hires_codes(data),
    }
}

/// The codes of one bitmap byte (REQ-318).
pub(super) fn mapped_codes(mode: u8, data: u8) -> Codes {
    match mode {
        8 => two_bit_codes(data, 4, false),
        9 => one_bit_codes(data, 2, AnCode::playfield(0)),
        0x0A => two_bit_codes(data, 2, false),
        0x0B | 0x0C => one_bit_codes(data, 1, AnCode::playfield(0)),
        0x0D | 0x0E => two_bit_codes(data, 1, false),
        _ => hires_codes(data),
    }
}

/// The font row of a character data fetch, or `None` if ANTIC substitutes `$00` data
/// (REQ-313, REQ-314, REQ-330). `row` is the row counter.
pub(super) const fn font_row(mode: u8, name: u8, row: u8, chactl: u8) -> Option<u8> {
    let descender = name & 0x60 == 0x60;
    let blank = match mode {
        2 => matches!(row, 8 | 9) && !descender,
        3 => {
            if descender {
                row < 2
            } else {
                matches!(row, 8 | 9)
            }
        }
        _ => false,
    };
    if blank {
        return None;
    }
    let frow = match mode {
        5 | 7 => (row >> 1) & 7,
        _ => row & 7,
    };
    Some(if chactl & 0x04 == 0 { frow } else { 7 - frow })
}

/// The font address of a character data fetch (HRM 4.4). `chbase` is the pipeline value.
pub(super) const fn font_address(mode: u8, name: u8, frow: u8, chbase: u8) -> u16 {
    let (base, index) = if matches!(mode, 6 | 7) {
        (chbase & 0xFE, name & 0x3F)
    } else {
        (chbase & 0xFC, name & 0x7F)
    };
    ((base as u16) << 8) | ((index as u16) << 3) | (frow as u16 & 7)
}

/// Applies `CHACTL` bits 0 and 1 to the data of a mode 2 or 3 character (REQ-314).
pub(super) const fn blink_invert(mode: u8, name: u8, data: u8, chactl: u8) -> u8 {
    if !matches!(mode, 2 | 3) || name & 0x80 == 0 {
        return data;
    }
    let shown = if chactl & 0x01 == 0 { data } else { 0x00 };
    if chactl & 0x02 == 0 { shown } else { !shown }
}

//! Playfield color tests (REQ-407).

use super::objects::{gtia_with, plain_frame};
use super::{COLBK, PRIOR, row};
use crate::antic::AnCode;

/// Verifies REQ-407: with `COLBK` = `$02`, `COLPF0-3` = `$14`, `$26`, `$38`, `$4A`, `PRIOR` =
/// `$00`, the last horizontal blank code `%010` and no P/M graphics, codes `%000`, `%100`,
/// `%101`, `%110`, `%111` at HPOS `$40-$44` give columns 64-73 = `$02`, `$02`, `$14`, `$14`,
/// `$26`, `$26`, `$38`, `$38`, `$4A`, `$4A` (criterion 1).
#[test]
fn lores_codes_map_to_color_registers() {
    let mut gtia = gtia_with(&[
        (COLBK, 0x02),
        (0x16, 0x14),
        (0x17, 0x26),
        (0x18, 0x38),
        (0x19, 0x4A),
        (PRIOR, 0x00),
    ]);
    let inner = |h| match h {
        0x41..=0x44 => AnCode::playfield(u8::try_from(h - 0x41).unwrap()),
        _ => AnCode::BACKGROUND,
    };
    plain_frame(&mut gtia, false, &inner);
    for r in [0, 100, 239] {
        assert_eq!(
            row(&gtia, r)[64..74],
            [0x02, 0x02, 0x14, 0x14, 0x26, 0x26, 0x38, 0x38, 0x4A, 0x4A],
            "row {r}"
        );
    }
}

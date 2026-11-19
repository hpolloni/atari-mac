//! Playfield color tests (REQ-407).

use crate::planned::planned;

/// Verifies REQ-407: with `COLBK` = `$02`, `COLPF0-3` = `$14`, `$26`, `$38`, `$4A`, `PRIOR` =
/// `$00`, the last horizontal blank code `%010` and no P/M graphics, codes `%000`, `%100`,
/// `%101`, `%110`, `%111` at HPOS `$40-$44` give columns 64-73 = `$02`, `$02`, `$14`, `$14`,
/// `$26`, `$26`, `$38`, `$38`, `$4A`, `$4A` (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn lores_codes_map_to_color_registers() {
    planned();
}

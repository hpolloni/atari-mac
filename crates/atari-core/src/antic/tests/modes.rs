//! Planned tests of the playfield codes and the character and mapped modes (REQ-306,
//! REQ-307, REQ-315 to REQ-318).

use crate::planned::planned;

/// Verifies REQ-306: on scan line 41 of a mode 2 line, positions `$00-$21` and `$DE-$E3`
/// have code `%011`; on scan line 61 of a mode A line, the same positions have `%010`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hblank_code_shows_hires_mode() {
    planned();
}

/// Verifies REQ-306: with IR = `$41` at scan line 248 (NTSC), every color clock of scan
/// lines 251-253 has code `%001`.
#[test]
#[ignore = "planned: SLICE-02"]
fn vsync_lines_ntsc_251_to_253() {
    planned();
}

/// Verifies REQ-306: with IR = `$41` at scan line 248 (PAL), every color clock of scan
/// lines 275-277 has code `%001`.
#[test]
#[ignore = "planned: SLICE-02"]
fn vsync_lines_pal_275_to_277() {
    planned();
}

/// Verifies REQ-306: with IR = `$41` at scan line 248, every color clock of the vertical
/// blank scan lines other than the vertical sync lines has code `%010` (NTSC and PAL).
#[test]
#[ignore = "planned: SLICE-02"]
fn vblank_lines_send_hblank_code() {
    planned();
}

/// Verifies REQ-306: with IR = `$02` at scan line 248 and `DMACTL` = `$20`, the vertical
/// blank scan lines have code `%011`, and the vertical sync scan lines have `%001`.
#[test]
#[ignore = "planned: SLICE-02"]
fn vblank_hires_ir_without_playfield_sends_011() {
    planned();
}

/// Verifies REQ-307: a normal-width mode F line with bytes `$1B` gives the codes `%100`,
/// `%101`, `%110` and `%111` in each group of 4 color clocks from `$30` to `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_mode_sends_pair_codes() {
    planned();
}

/// Verifies REQ-307: a mode F line with bytes `$00`, and a mode 2 line with font data
/// `$00`, have code `%100` at every position from `$30` to `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn hires_zero_pair_is_100() {
    planned();
}

/// Verifies REQ-307: a normal-width mode E line with bytes `$1B` gives the codes `%000`,
/// `%100`, `%101` and `%110` in each group of 4 color clocks from `$30` to `$CF`.
#[test]
#[ignore = "planned: SLICE-02"]
fn lores_background_is_000() {
    planned();
}

/// Verifies REQ-307: on the mode F and mode E lines, positions `$22-$2F` and `$D0-$DD` have
/// code `%000`.
#[test]
#[ignore = "planned: SLICE-02"]
fn border_is_background() {
    planned();
}

/// Verifies REQ-307 and REQ-315: in a mode 4 line, a character with name `$81` and font
/// byte `$FF` has code `%111` (`PF3`) on its 4 color clocks; with font byte `$1B` it has
/// the codes `%000`, `%100`, `%101` and `%111`.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_4_bit7_selects_pf3() {
    planned();
}

/// Verifies REQ-315: in a mode 4 line, a character with name `$01` and font byte `$1B` has
/// the codes `%000`, `%100`, `%101` and `%110`, left to right.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_4_color_pairs() {
    planned();
}

/// Verifies REQ-315: with `CHBASE` = `$E0`, the row 0 font fetch of a mode 4 character with
/// name `$C1` reads `$E208`.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_4_font_address() {
    planned();
}

/// Verifies REQ-315: in a normal-width mode 4 line with names `$00-$27`, positions
/// `$30 + 4k` to `$33 + 4k` show the font byte of name k, for k = 0 to 39.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_4_forty_characters() {
    planned();
}

/// Verifies REQ-315: in mode 4, row r (0-7) reads font row r; in mode 5, row r (0-15)
/// reads font row r >> 1.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_5_double_rows() {
    planned();
}

/// Verifies REQ-316: in a mode 6 line, name `$01` with font byte `$80` has code `%100` on
/// its first color clock and `%000` on its other 7 color clocks.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_6_zero_bit_is_background() {
    planned();
}

/// Verifies REQ-316: with font byte `$80`, the names `$01`, `$41`, `$81` and `$C1` give
/// code `%100`, `%101`, `%110` and `%111` on the first color clock.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_6_color_from_name_bits() {
    planned();
}

/// Verifies REQ-316: in a normal-width mode 6 line with names `$00-$13`, positions
/// `$30 + 8k` to `$37 + 8k` show the font byte of name k, for k = 0 to 19.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_6_twenty_characters() {
    planned();
}

/// Verifies REQ-316: with `CHBASE` = `$E2` and name `$C1`, the row 0 font fetch reads
/// `$E208`.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_6_font_address_512_aligned() {
    planned();
}

/// Verifies REQ-316: in mode 6, row r (0-7) reads font row r; in mode 7, row r (0-15)
/// reads font row r >> 1.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_7_double_rows() {
    planned();
}

/// Verifies REQ-317: in a normal-width mode 2 line with `CHBASE` = `$E0`, a write of `$E4`
/// on cycle 50 does not change the character data fetch on cycle 51 (it reads in
/// `$E000-$E3FF`).
#[test]
#[ignore = "planned: SLICE-02"]
fn chbase_write_one_cycle_before_is_too_late() {
    planned();
}

/// Verifies REQ-317: a write of `$E4` to `CHBASE` on cycle 49 changes the character data
/// fetch on cycle 51 (it reads in `$E400-$E7FF`).
#[test]
#[ignore = "planned: SLICE-02"]
fn chbase_takes_effect_after_two_cycles() {
    planned();
}

/// Verifies REQ-317: after a write of `$E2` to `CHBASE`, mode 2 name `$00` reads row 0 from
/// `$E000`, and in the next mode line, a mode 6 line, name `$00` reads row 0 from `$E200`
/// with no new write.
#[test]
#[ignore = "planned: SLICE-02"]
fn chbase_bit1_latent_for_mode_6() {
    planned();
}

/// Verifies REQ-318: in modes 8, A, D and E, a byte `$1B` gives the codes `%000`, `%100`,
/// `%101` and `%110`, with pixels 4 (mode 8), 2 (mode A) or 1 (modes D, E) color clocks
/// wide.
#[test]
#[ignore = "planned: SLICE-02"]
fn mapped_two_bit_encoding() {
    planned();
}

/// Verifies REQ-318: in modes 9, B and C, a byte `$55` gives the codes `%000` and `%100` in
/// turn, with pixels 2 (mode 9) or 1 (modes B, C) color clocks wide.
#[test]
#[ignore = "planned: SLICE-02"]
fn mapped_one_bit_encoding() {
    planned();
}

/// Verifies REQ-318: in mode F, a byte `$1B` gives the codes `%100`, `%101`, `%110` and
/// `%111`, each for 1 color clock.
#[test]
#[ignore = "planned: SLICE-02"]
fn mode_f_hires_pairs() {
    planned();
}

/// Verifies REQ-318: a normal-width line fetches 10 bytes in modes 8 and 9, 20 in modes A
/// to C and 40 in modes D to F, and byte k starts at `$30 + 16k`, `$30 + 8k` or `$30 + 4k`.
#[test]
#[ignore = "planned: SLICE-02"]
fn mapped_bytes_per_line() {
    planned();
}

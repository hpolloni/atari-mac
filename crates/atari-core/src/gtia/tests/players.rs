//! Player and missile graphics tests: data, DMA, position, size and shifting (REQ-413 to
//! REQ-419, REQ-441).

use crate::planned::planned;

/// Verifies REQ-413: `GRAFP0` = `$81`, `HPOSP0` = `$80`, `SIZEP0` = `$00`, `COLPM0` = `$46`
/// show `$46` at HPOS `$80` and `$87` and `$02` at HPOS `$81-$86` (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn player_bits_msb_first() {
    planned();
}

/// Verifies REQ-413: missile n shows bits `2n+1` and `2n` of `GRAFM` in `COLPMn`, the most
/// significant bit at the left (criteria 2 and 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn missile_bits_and_colors() {
    planned();
}

/// Verifies REQ-413: players 1-3 and missiles 2-3 at different positions show `COLPM1-3`
/// (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn all_object_colors() {
    planned();
}

/// Verifies REQ-413: with P/M DMA off and one write of `$80` to `GRAFP0` before scanline 8,
/// columns 192-193 of all 240 rows are `COLPM0` (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn latch_repeats_without_dma() {
    planned();
}

/// Verifies REQ-414: writes `$00` to `GRACTL` in cycle 110 of scanline 99 and checks that
/// rows 92-239 all show `$46` at HPOS `$81` only (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn gractl_off_keeps_latch() {
    planned();
}

/// Verifies REQ-414: with `GRACTL` = `$00` and again with `$03`, a CPU write of `$80` to
/// `GRAFP0` in horizontal blank after the player DMA cycle shows the player on that scanline
/// (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn cpu_writes_ignore_gractl() {
    planned();
}

/// Verifies REQ-415: sets each `VDELAY` bit alone and checks that only the related object
/// skips the even-scanline DMA byte (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn vdelay_bit_map() {
    planned();
}

/// Verifies REQ-415: with `VDELAY` = `$10` and a player 0 DMA byte of `$00`, a CPU write of
/// `$80` to `GRAFP0` in horizontal blank of an even scanline shows the player at HPOS `$80`
/// (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn vdelay_ignores_cpu_writes() {
    planned();
}

/// Verifies REQ-416: `GRAFP0` = `$80` and `HPOSP0` = `$80` show `$46` at HPOS `$80` (columns
/// 192-193) and `$02` at HPOS `$7F` and `$81` (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn player_starts_at_hpos() {
    planned();
}

/// Verifies REQ-416: `HPOSP0` = `$1E`, `$DA` and `$DE` clip the image at the left and right
/// edges of `$22-$DD`, and the image does not continue on the next scanline (criteria 2-4).
#[test]
#[ignore = "planned: SLICE-02"]
fn player_clipped_at_left_and_right() {
    planned();
}

/// Verifies REQ-417: `SIZEP0` = `$00`, `$01`, `$03`, `$02` show each bit for 1, 2, 4 and 1
/// color clocks, and `GRAFP0` = `$FF` at quadruple size covers HPOS `$40-$5F` (criteria 1 and
/// 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn player_sizes() {
    planned();
}

/// Verifies REQ-417: `GRAFM` = `$08`, `HPOSM1` = `$90` and `SIZEM` = `$0C` show `COLPM1` at
/// HPOS `$90-$93` and `COLBK` at HPOS `$94` (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn missile_sizes() {
    planned();
}

/// Verifies REQ-417: compares the rows for `SIZEP0` = `$FC` and `$00`, and `$FD` and `$01`
/// (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn size_bits_2_7_ignored() {
    planned();
}

/// Verifies REQ-418: `HPOSP0` = `$40` and a write of `$A0` that takes effect at HPOS `$67`
/// show the player at HPOS `$40-$47` and `$A0-$A7` only (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn retrigger_shows_second_copy() {
    planned();
}

/// Verifies REQ-418: `HPOSP0` = `$A0` and a write of `$40` that takes effect at HPOS `$67`
/// show no player on that scanline (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn move_left_suppresses_image() {
    planned();
}

/// Verifies REQ-418: `HPOSP0` = `$60` and a write of `$50` that takes effect at HPOS `$63`
/// show the player at HPOS `$60-$67` and `COLBK` at HPOS `$68-$DD` (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn move_during_image_keeps_image() {
    planned();
}

/// Verifies REQ-418: `GRAFP0` = `$A0`, `HPOSP0` = `$60` and a write of `$61` that takes
/// effect at HPOS `$61` show the player at HPOS `$60-$63` only: the load ORs the shifted old
/// bits with the new bits (criterion 4).
#[test]
#[ignore = "planned: SLICE-02"]
fn reload_ors_shift_register() {
    planned();
}

/// Verifies REQ-418: `HPOSP0` = `$1A` shows `COLBK` at HPOS `$22-$DD`: the image shifts out
/// in horizontal blank (criterion 5).
#[test]
#[ignore = "planned: SLICE-02"]
fn image_shifts_out_in_hblank() {
    planned();
}

/// Verifies REQ-419: for each of the 20 rows of HRM Table 15, the next four shift states
/// after the size write are the four "after" values of the row (criterion 1).
#[test]
#[ignore = "planned: SLICE-02"]
fn size_change_table_15() {
    planned();
}

/// Verifies REQ-419: `SIZEP0` `$01` to `$02` at HPOS `$41` shows `COLPM0` from HPOS `$40` to
/// the end of the scanline and at HPOS `$22-$3F` of the next scanline, also from scanline 247
/// to scanline 8 of the next frame (criterion 2).
#[test]
#[ignore = "planned: SLICE-02"]
fn size_10_lockup_persists() {
    planned();
}

/// Verifies REQ-419: changes `SIZEP0` after the image and compares the row with a row with
/// no write (criterion 3).
#[test]
#[ignore = "planned: SLICE-02"]
fn size_change_after_image() {
    planned();
}

/// Verifies REQ-441: feeds `CycleOutput` values with a missile DMA cycle and bus values on
/// cycles 2-5, and checks the player images; with `GRACTL` = `$01` `GRAFP0-3` do not change,
/// and real player DMA bytes take precedence over the bus values (criteria 1-3).
#[test]
#[ignore = "planned: SLICE-02"]
fn players_load_bus_without_player_dma() {
    planned();
}
